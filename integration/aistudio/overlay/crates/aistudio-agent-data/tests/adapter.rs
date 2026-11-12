//! Compile the production Tauri Agent adapter with a small shell stand-in;
//! DesktopApi, BuildIdentity, account and SQLite remain the real native owners.
extern crate self as tauri;
use aistudio_account::{AccountOptions, AccountService, SecretCipher};
use aistudio_desktop_api::{DesktopApi, DesktopEffects, EffectFuture, SaveDialog};
use aistudio_provider_io::{HttpPolicy, ProviderClient, Route};
use aistudio_runtime_backend::RuntimeBackend;
use aistudio_storage::{StorageContext, StorageWriter};
use interprocess::local_socket::{prelude::*, GenericFilePath, Stream};
use serde_json::{json, Value};
use std::{
    any::Any,
    fs,
    io::{BufRead, BufReader, Write},
    ops::Deref,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
#[path = "../../../src-tauri/src/agent_service.rs"]
mod agent_service;
#[path = "../../../src-tauri/src/build_identity.rs"]
mod build_identity;
pub mod async_runtime {
    pub use tokio::task::{spawn_blocking, JoinHandle};
}
#[derive(Clone)]
pub struct AppHandle {
    state: Arc<dyn Any + Send + Sync>,
}
pub struct State<'a, T>(&'a T);
impl<T> Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0
    }
}
pub trait Manager {
    fn try_state<T: Any + Send + Sync>(&self) -> Option<State<'_, T>>;
}
impl Manager for AppHandle {
    fn try_state<T: Any + Send + Sync>(&self) -> Option<State<'_, T>> {
        self.state.downcast_ref::<T>().map(State)
    }
}
#[derive(Default)]
struct Pause {
    state: Mutex<(bool, bool)>,
    ready: Condvar,
}
impl Pause {
    fn hold(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.ready.notify_all();
        while !state.1 {
            state = self.ready.wait(state).unwrap();
        }
    }
    fn wait(&self) {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .ready
            .wait_timeout_while(state, Duration::from_secs(3), |state| !state.0)
            .unwrap();
        assert!(state.0);
    }
    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.ready.notify_all();
    }
}
pub mod desktop_service {
    use super::*;
    pub struct DesktopService {
        pub api: Arc<DesktopApi>,
        pub builds: Mutex<build_identity::BuildIdentity>,
        pub account: Arc<AccountService>,
        pub guards: Mutex<Vec<String>>,
        pub changes: Mutex<Vec<Value>>,
        pub(super) pause: Mutex<Option<Arc<Pause>>>,
    }
    impl DesktopService {
        pub fn verified(&self, client: &str, value: &Value) -> Value {
            self.builds.lock().unwrap().verify(client, value)
        }
        pub fn guard_write(&self, client: &str, channel: &str) -> std::result::Result<(), Value> {
            self.guards.lock().unwrap().push(client.into());
            if build_identity::read_only(channel) {
                return Ok(());
            }
            self.account
                .assert_local()
                .map_err(|error| json!({"code":error.code,"message":error.message,"details":{}}))?;
            self.builds.lock().unwrap().assert_writable(client)
        }
        pub async fn invoke(
            &self,
            channel: String,
            args: Vec<Value>,
            undefined: Vec<String>,
        ) -> std::result::Result<Value, Value> {
            if channel == "data:apply" && args[0]["commandId"] == "slow" {
                let pause = self.pause.lock().unwrap().clone();
                if let Some(pause) = pause {
                    pause.hold();
                }
            }
            let api = self.api.clone();
            let runtime = tokio::runtime::Handle::current();
            let reply = tokio::task::spawn_blocking(move || {
                runtime.block_on(api.invoke(&channel, &args, &undefined))
            })
            .await
            .unwrap();
            self.changes.lock().unwrap().extend(reply.events);
            Ok(reply.value)
        }
    }
}
struct Cipher;
impl SecretCipher for Cipher {
    fn available(&self) -> bool {
        true
    }
    fn encrypt(&self, clear: &str) -> aistudio_config_store::Result<Vec<u8>> {
        Ok(clear.bytes().map(|b| b ^ 0xa5).collect())
    }
    fn decrypt(&self, bytes: &[u8]) -> aistudio_config_store::Result<String> {
        String::from_utf8(bytes.iter().map(|b| b ^ 0xa5).collect())
            .map_err(|_| aistudio_config_store::error("SECRET_DECRYPT_FAILED", "fixture"))
    }
}
struct Effects;
impl DesktopEffects for Effects {
    fn choose_save(&self, _: SaveDialog) -> EffectFuture<'_, Option<PathBuf>> {
        Box::pin(async { panic!("Agent never opens dialogs") })
    }
    fn reveal<'a>(&'a self, _: &'a Path) -> EffectFuture<'a, ()> {
        Box::pin(async { panic!("Agent never reveals paths") })
    }
    fn open_external<'a>(&'a self, _: &'a str, _: bool) -> EffectFuture<'a, Value> {
        Box::pin(async { panic!("Agent never opens URLs") })
    }
    fn open_tutorial<'a>(&'a self, _: &'a str) -> EffectFuture<'a, ()> {
        Box::pin(async { panic!("Agent never opens tutorials") })
    }
}
struct Fixture {
    root: PathBuf,
    backend: Arc<RuntimeBackend>,
    writer: Arc<StorageWriter>,
    desktop: Arc<desktop_service::DesktopService>,
    app: AppHandle,
}
impl Fixture {
    async fn new() -> Self {
        // Explicit compatibility fixture, not the application's default mode.
        std::env::set_var("AISTUDIO_AGENT_LEGACY_V1", "1");
        let root =
            std::env::temp_dir().join(format!("native-agent-adapter-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("account")).unwrap();
        let session = json!({"version":1,"serviceUrl":"http://127.0.0.1:9","cookies":{"session":"fixture-only"},"profile":{"id":"u","email":"u@example.test","emailVerified":true,"everActivated":true,"validUntil":"2099-01-01T00:00:00Z","cloudEligible":true,"localExpiryPolicy":"pending"}});
        fs::write(
            root.join("account/account-session.enc"),
            Cipher.encrypt(&session.to_string()).unwrap(),
        )
        .unwrap();
        let account = Arc::new(
            AccountService::new(
                AccountOptions {
                    directory: root.join("account"),
                    media_directory: root.join("media-v4"),
                    default_service_url: "http://127.0.0.1:9".into(),
                    timeout: Duration::from_secs(1),
                    media_idle_timeout: Duration::from_secs(1),
                },
                Arc::new(Cipher),
                ProviderClient::new(Route::Direct, HttpPolicy::default()).unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(account.load()["localAllowed"], true);
        let writer = Arc::new(StorageWriter::open(StorageContext::new(root.clone())).unwrap());
        let backend = Arc::new(
            RuntimeBackend::start(writer.clone(), account.snapshot(), Arc::new(Cipher)).unwrap(),
        );
        let api = Arc::new(DesktopApi::new(
            root.clone(),
            backend.clone(),
            account.clone(),
            Arc::new(Effects),
        ));
        api.invoke_result("local-data:drafts-create",&[json!({"id":"d","title":"fixture","storyboard":[{"id":"s","remark":"before"}],"episodes":[],"extractedAssets":[],"script":[]})],&[]).await.unwrap();
        let mut builds = build_identity::BuildIdentity::new("test+1".into());
        builds.set_host(&json!("test+1"));
        builds.verify("main", &json!("test+1"));
        let desktop = Arc::new(desktop_service::DesktopService {
            api,
            builds: Mutex::new(builds),
            account,
            guards: Mutex::new(vec![]),
            changes: Mutex::new(vec![]),
            pause: Mutex::new(None),
        });
        let app = AppHandle {
            state: desktop.clone(),
        };
        Self {
            root,
            backend,
            writer,
            desktop,
            app,
        }
    }
    fn service(&self, sink: agent_service::OperationSink) -> agent_service::AgentService {
        agent_service::AgentService::new(
            self.app.clone(),
            self.root.clone(),
            std::env::temp_dir(),
            sink,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.backend.shutdown().unwrap();
        self.writer.close().unwrap();
        let _ = fs::remove_dir_all(&self.root);
    }
}
struct Client {
    stream: BufReader<Stream>,
    descriptor: aistudio_agent_data::Descriptor,
}
impl Client {
    fn new(root: &Path) -> Self {
        let descriptor: aistudio_agent_data::Descriptor =
            serde_json::from_slice(&fs::read(root.join("agent-data-connection.json")).unwrap())
                .unwrap();
        let stream = Stream::connect(
            descriptor
                .endpoint
                .as_str()
                .to_fs_name::<GenericFilePath>()
                .unwrap(),
        )
        .unwrap();
        Self {
            stream: BufReader::new(stream),
            descriptor,
        }
    }
    fn send(&mut self, method: &str, params: Value) {
        writeln!(
            self.stream.get_mut(),
            "{}",
            json!({"id":1,"token":self.descriptor.token,"method":method,"params":params})
        )
        .unwrap();
        self.stream.get_mut().flush().unwrap();
    }
    fn reply(&mut self) -> Value {
        let mut line = String::new();
        self.stream.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    fn hello(&mut self, build: &str) -> Value {
        self.send(
            "hello",
            json!({"protocol":1,"buildId":build,"dataRoot":self.descriptor.data_root}),
        );
        self.reply()
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adapter_constructs_idle_and_uses_existing_api_identity_guard_events_and_operation_sink() {
    let f = Fixture::new().await;
    let records = Arc::new(Mutex::new(vec![]));
    let saved = records.clone();
    let service = f.service(Arc::new(move |kind, details| {
        saved.lock().unwrap().push((kind.to_owned(), details))
    }));
    let descriptor = f.root.join("agent-data-connection.json");
    assert!(!descriptor.exists());
    let (first, second) = tokio::join!(service.start(), service.start());
    first.unwrap();
    second.unwrap();
    let mut bad = Client::new(&f.root);
    assert_eq!(bad.hello("wrong+2")["error"]["code"], "BUILD_MISMATCH");
    drop(bad);
    assert_eq!(
        f.desktop.builds.lock().unwrap().snapshot("main")["writable"],
        true
    );
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/client-compat.cjs");
    let output = Command::new("node")
        .arg(script)
        .arg(&f.root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        f.desktop
            .changes
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event["entity"] == "shot")
            .count(),
        1,
        "adapter must publish each commit only through DesktopService.invoke"
    );
    assert!(f
        .desktop
        .guards
        .lock()
        .unwrap()
        .iter()
        .all(|client| client.starts_with("agent:") && client != "main"));
    let payload = fs::read(&descriptor).unwrap();
    let token = serde_json::from_slice::<Value>(&payload).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let records = records.lock().unwrap();
    assert!(!records.is_empty());
    assert!(records.iter().all(|(kind, _)| kind == "agent.command"));
    let captured = serde_json::to_string(&*records).unwrap();
    assert!(!captured.contains(&token));
    assert!(!captured.contains("Node CLI → Rust SQLite"));
    drop(records);
    service.close().await.unwrap();
    service.close().await.unwrap();
    assert!(!descriptor.exists());
    assert_eq!(
        service.start().await.unwrap_err().code,
        "AGENT_SERVICE_CLOSED"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adapter_cancelled_close_retry_keeps_waiting_for_the_accepted_sqlite_write() {
    let f = Fixture::new().await;
    let pause = Arc::new(Pause::default());
    *f.desktop.pause.lock().unwrap() = Some(pause.clone());
    let service = f.service(Arc::new(|_, _| {}));
    service.start().await.unwrap();
    let mut client = Client::new(&f.root);
    assert_eq!(client.hello("test+1")["ok"], true);
    client.send("draft.apply",json!({"commandId":"slow","draftId":"d","commands":[{"op":"patchShot","shotId":"s","baseVersion":1,"fields":{"remark":"accepted-before-close"}}]}));
    pause.wait();
    drop(client);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), service.close())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(20), service.close())
            .await
            .is_err(),
        "a cancelled first close must not lose its running drain owner"
    );
    pause.release();
    service.close().await.unwrap();
    let draft = f
        .backend
        .dispatch_v4_trusted("draft.get", json!({"id":"d"}), None, &[])
        .unwrap()
        .value;
    assert_eq!(draft["storyboard"][0]["remark"], "accepted-before-close");
    assert!(!f.root.join("agent-data-connection.json").exists());
}

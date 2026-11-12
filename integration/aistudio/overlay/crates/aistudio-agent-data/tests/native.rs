use aistudio_account::{AccountOptions, AccountService, SecretCipher};
use aistudio_agent_data::*;
use aistudio_desktop_api::{DesktopApi, DesktopEffects, EffectFuture, SaveDialog};
use aistudio_provider_io::{HttpPolicy, ProviderClient, Route};
use aistudio_runtime_backend::RuntimeBackend;
use aistudio_storage::{StorageContext, StorageWriter};
use interprocess::local_socket::{prelude::*, GenericFilePath, Stream};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    time::Duration,
};
#[path = "../../../src-tauri/src/build_identity.rs"]
mod build_identity;
struct Cipher;
impl SecretCipher for Cipher {
    fn available(&self) -> bool {
        true
    }
    fn encrypt(&self, clear: &str) -> aistudio_config_store::Result<Vec<u8>> {
        Ok(clear.bytes().map(|byte| byte ^ 0xa5).collect())
    }
    fn decrypt(&self, bytes: &[u8]) -> aistudio_config_store::Result<String> {
        String::from_utf8(bytes.iter().map(|byte| byte ^ 0xa5).collect())
            .map_err(|_| aistudio_config_store::error("SECRET_DECRYPT_FAILED", "fixture"))
    }
}
struct Effects;
impl DesktopEffects for Effects {
    fn choose_save(&self, _: SaveDialog) -> EffectFuture<'_, Option<PathBuf>> {
        Box::pin(async { panic!("Agent has no dialog route") })
    }
    fn reveal<'a>(&'a self, _: &'a Path) -> EffectFuture<'a, ()> {
        Box::pin(async { panic!("Agent has no reveal route") })
    }
    fn open_external<'a>(&'a self, _: &'a str, _: bool) -> EffectFuture<'a, Value> {
        Box::pin(async { panic!("Agent has no external URL route") })
    }
    fn open_tutorial<'a>(&'a self, _: &'a str) -> EffectFuture<'a, ()> {
        Box::pin(async { panic!("Agent has no tutorial route") })
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
        assert!(state.0, "native invoke was not accepted");
    }
    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.ready.notify_all();
    }
}
struct NativeHost {
    api: DesktopApi,
    builds: Mutex<build_identity::BuildIdentity>,
    verifies: AtomicUsize,
    calls: AtomicUsize,
    updating: AtomicBool,
    pause: Mutex<Option<Arc<Pause>>>,
    changes: Mutex<Vec<Value>>,
    operations: Mutex<Vec<Value>>,
}
fn host_error(value: Value) -> Error {
    let mut error = Error::new(
        value["code"].as_str().unwrap_or("AGENT_COMMAND_FAILED"),
        value["message"]
            .as_str()
            .unwrap_or("Application command failed"),
    );
    error.details = value.get("details").cloned().unwrap_or_else(|| json!({}));
    error
}
impl Host for NativeHost {
    fn build_snapshot(&self, client: &str) -> Value {
        self.builds.lock().unwrap().snapshot(client)
    }
    fn verify_build(&self, client: &str, build: &str) -> Result<Value> {
        self.verifies.fetch_add(1, Ordering::SeqCst);
        Ok(self.builds.lock().unwrap().verify(client, &json!(build)))
    }
    fn forget_build(&self, client: &str) {
        self.builds.lock().unwrap().forget(client);
    }
    fn invoke<'a>(
        &'a self,
        channel: &'a str,
        client: &'a str,
        args: Vec<Value>,
    ) -> HostFuture<'a, Value> {
        Box::pin(async move {
            assert!(client.starts_with("agent:"));
            if !build_identity::read_only(channel) {
                self.builds
                    .lock()
                    .unwrap()
                    .assert_writable(client)
                    .map_err(host_error)?;
                if self.updating.load(Ordering::SeqCst) {
                    return Err(Error::new("UPDATE_INSTALL_PENDING", "installing update"));
                }
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            if channel == "data:apply" && args[0]["commandId"] == "slow" {
                let pause = self.pause.lock().unwrap().clone();
                if let Some(pause) = pause {
                    pause.hold();
                }
            }
            let reply = self.api.invoke(channel, &args, &[]).await;
            self.changes.lock().unwrap().extend(reply.events);
            Ok(reply.value)
        })
    }
    fn operation(&self, operation: Operation) {
        self.operations
            .lock()
            .unwrap()
            .push(serde_json::to_value(operation).unwrap());
    }
}
struct Fixture {
    root: PathBuf,
    backend: Arc<RuntimeBackend>,
    writer: Arc<StorageWriter>,
    host: Arc<NativeHost>,
}
impl Fixture {
    fn new() -> Self {
        // This test binary specifically exercises the legacy developer CLI.
        // Production defaults and v2 tests remain approval-only.
        std::env::set_var("AISTUDIO_AGENT_LEGACY_V1", "1");
        let root = std::env::temp_dir().join(format!("native-agent-{}", uuid::Uuid::new_v4()));
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
        let api = DesktopApi::new(root.clone(), backend.clone(), account, Arc::new(Effects));
        let mut gate = build_identity::BuildIdentity::new("test+1".into());
        gate.set_host(&json!("test+1"));
        gate.verify("main", &json!("test+1"));
        let host = Arc::new(NativeHost {
            api,
            builds: Mutex::new(gate),
            verifies: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            updating: AtomicBool::new(false),
            pause: Mutex::new(None),
            changes: Mutex::new(vec![]),
            operations: Mutex::new(vec![]),
        });
        Self {
            root,
            backend,
            writer,
            host,
        }
    }
    async fn draft(&self) {
        self.host.api.invoke_result("local-data:drafts-create", &[json!({"id":"d","title":"fixture","episodes":[{"id":"e"}],"storyboard":[{"id":"s","episodeId":"e","remark":"before"}],"extractedAssets":[],"script":[]})], &[]).await.unwrap();
    }
    fn server(&self) -> Server {
        Server::start(
            Options {
                data_root: self.root.clone(),
                temporary_directory: std::env::temp_dir(),
            },
            self.host.clone(),
            tokio::runtime::Handle::current(),
        )
        .unwrap()
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
    descriptor: Descriptor,
    sequence: usize,
}
impl Client {
    fn connect(descriptor: &Descriptor) -> Self {
        let stream = Stream::connect(
            descriptor
                .endpoint
                .as_str()
                .to_fs_name::<GenericFilePath>()
                .unwrap(),
        )
        .unwrap();
        #[cfg(unix)]
        stream
            .set_recv_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        Self {
            stream: BufReader::new(stream),
            descriptor: descriptor.clone(),
            sequence: 0,
        }
    }
    fn send(&mut self, method: &str, params: Value) {
        self.sequence += 1;
        self.raw(json!({"id":self.sequence,"token":self.descriptor.token,"method":method,"params":params}));
    }
    fn raw(&mut self, request: Value) {
        writeln!(self.stream.get_mut(), "{request}").unwrap();
        self.stream.get_mut().flush().unwrap();
    }
    fn reply(&mut self) -> Value {
        let mut line = String::new();
        self.stream.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.send(method, params);
        self.reply()
    }
    fn hello(&mut self) -> Value {
        self.request(
            "hello",
            json!({"protocol":1,"buildId":"test+1","dataRoot":self.descriptor.data_root}),
        )
    }
}
fn edit(id: &str, base: i64, remark: &str) -> Value {
    json!({"commandId":id,"draftId":"d","commands":[{"op":"patchShot","shotId":"s","baseVersion":base,"fields":{"remark":remark}}]})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_connection_survives_between_requests_and_disconnects_cleanly() {
    let f = Fixture::new();
    f.draft().await;
    let mut server = f.server();
    let mut client = Client::connect(server.descriptor());
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(client.hello()["ok"], true);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(client.request("draft.get", json!({"draftId":"d"}))["ok"], true);
    drop(client);
    let started = std::time::Instant::now();
    server.close().unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_style_reference_update_uses_the_application_settings_writer() {
    let f = Fixture::new();
    f.backend.call("config.kv.set", json!({"namespace":"settings","key":"app","value":{
        "globalStyleLibrary":{"styles":[
            {"id":"style-a","name":"甲","refImages":["old-a"]},
            {"id":"style-b","name":"乙","refImages":["old-b"]}
        ]},"imageModel":"keep"}}), 1).unwrap();
    let mut server = f.server();
    let mut client = Client::connect(server.descriptor());
    client.hello();
    let updated = client.request("style.setReferences", json!({
        "styleId":"style-a","refImages":["new-a","new-b"]
    }));
    assert_eq!(updated["result"]["refImages"], json!(["new-a","new-b"]));
    let saved = f.backend.call("config.kv.get", json!({"namespace":"settings","key":"app"}), 1).unwrap().value;
    assert_eq!(saved["globalStyleLibrary"]["styles"][0]["refImages"], json!(["new-a","new-b"]));
    assert_eq!(saved["globalStyleLibrary"]["styles"][1]["refImages"], json!(["old-b"]));
    assert_eq!(saved["imageModel"], "keep");
    drop(client);
    server.close().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_socket_uses_same_sqlite_receipts_public_views_guards_and_cli() {
    let f = Fixture::new();
    f.draft().await;
    let mut server = f.server();
    let descriptor = server.descriptor().clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [
            f.root.join("agent-data-connection.json"),
            PathBuf::from(&descriptor.endpoint),
        ] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    assert!(!format!("{descriptor:?}").contains(&descriptor.token));
    let mut client = Client::connect(&descriptor);
    assert_eq!(
        client.request("draft.get", json!({"draftId":"d"}))["error"]["code"],
        "AGENT_HANDSHAKE_REQUIRED"
    );
    let hello = client.hello();
    assert_eq!(hello["result"]["build"]["writable"], true);
    assert!(hello["result"]["sessionId"]
        .as_str()
        .unwrap()
        .starts_with("agent:"));
    assert_ne!(hello["result"]["sessionId"], "main");
    assert_eq!(client.hello()["error"]["code"], "AGENT_ALREADY_CONNECTED");
    assert_eq!(
        client.request("shell.open-path", json!({}))["error"]["code"],
        "AGENT_METHOD_NOT_ALLOWED"
    );
    assert_eq!(
        client.request(
            "task.intent",
            json!({"requestId":"r","options":{"manual":true}})
        )["error"]["code"],
        "AGENT_MANUAL_NOT_ALLOWED"
    );
    assert_eq!(
        client.request("draft.apply", edit("native-original", 1, "after"))["result"]["replayed"],
        false
    );
    assert_eq!(
        client.request("draft.apply", edit("native-original", 1, "after"))["result"]["replayed"],
        true
    );
    let conflict = client.request("draft.apply", edit("stale", 1, "must rollback"));
    assert_eq!(conflict["error"]["code"], "FIELD_CONFLICT");
    assert_eq!(
        client.request("draft.get", json!({"draftId":"d"}))["result"]["storyboard"][0]["remark"],
        "after"
    );
    f.host.updating.store(true, Ordering::SeqCst);
    assert_eq!(
        client.request("draft.apply", edit("blocked", 2, "must not write"))["error"]["code"],
        "UPDATE_INSTALL_PENDING"
    );
    assert_eq!(
        client.request("draft.get", json!({"draftId":"d"}))["ok"],
        true
    );
    f.host.updating.store(false, Ordering::SeqCst);
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
    assert!(!serde_json::to_string(&*f.host.operations.lock().unwrap())
        .unwrap()
        .contains(&descriptor.token));
    drop(client);
    server.close().unwrap();
    assert!(!f.root.join("agent-data-connection.json").exists());
    #[cfg(unix)]
    assert!(!Path::new(&descriptor.endpoint).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_bad_handshakes_do_not_poison_windows_and_session_identity_is_forgotten() {
    let f = Fixture::new();
    let mut server = f.server();
    let descriptor = server.descriptor().clone();
    let mut bad = Client::connect(&descriptor);
    bad.stream.get_mut().write_all(b"{bad}\n").unwrap();
    assert_eq!(bad.reply()["error"]["code"], "AGENT_INVALID_REQUEST");
    bad.raw(json!({"id":1,"token":"wrong","method":"hello","params":{}}));
    assert_eq!(bad.reply()["error"]["code"], "AGENT_UNAUTHORIZED");
    for (patch, code) in [
        (json!({"protocol":2}), "AGENT_PROTOCOL_MISMATCH"),
        (json!({"dataRoot":"/different"}), "AGENT_DATA_ROOT_MISMATCH"),
        (json!({"buildId":"bad+2"}), "BUILD_MISMATCH"),
    ] {
        let mut params = json!({"protocol":1,"buildId":"test+1","dataRoot":f.root});
        for (key, value) in patch.as_object().unwrap() {
            params[key] = value.clone();
        }
        assert_eq!(bad.request("hello", params)["error"]["code"], code);
    }
    assert_eq!(f.host.verifies.load(Ordering::SeqCst), 0);
    assert_eq!(f.host.build_snapshot("main")["writable"], true);
    let hello = bad.hello();
    let client = hello["result"]["sessionId"].as_str().unwrap().to_owned();
    assert_eq!(hello["ok"], true);
    assert_eq!(f.host.build_snapshot(&client)["writable"], true);
    drop(bad);
    server.close().unwrap();
    assert_eq!(f.host.build_snapshot(&client)["renderer"], Value::Null);
    assert_eq!(f.host.build_snapshot("main")["writable"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_connection_serializes_methods_and_close_drains_disconnected_accepted_write() {
    let f = Fixture::new();
    f.draft().await;
    let server = f.server();
    let descriptor = server.descriptor().clone();
    let pause = Arc::new(Pause::default());
    *f.host.pause.lock().unwrap() = Some(pause.clone());
    let mut client = Client::connect(&descriptor);
    client.hello();
    client.send("draft.apply", edit("slow", 1, "accepted"));
    pause.wait();
    client.send("draft.apply", edit("after-slow", 2, "second"));
    let accepted = f.host.calls.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        f.host.calls.load(Ordering::SeqCst),
        accepted,
        "same connection must serialize application methods"
    );
    drop(client);
    let (done_tx, done_rx) = mpsc::channel();
    let closing = std::thread::spawn(move || {
        let mut server = server;
        let result = server.close();
        done_tx.send(()).unwrap();
        result
    });
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    pause.release();
    closing.join().unwrap().unwrap();
    assert_eq!(
        f.host.calls.load(Ordering::SeqCst),
        accepted,
        "close must not start the next queued method"
    );
    let saved = f
        .backend
        .dispatch_v4_trusted("draft.get", json!({"id":"d"}), None, &[])
        .unwrap()
        .value;
    assert_eq!(saved["storyboard"][0]["remark"], "accepted");
    assert_eq!(
        f.host
            .changes
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event["entity"] == "shot")
            .count(),
        1
    );
    let mut restarted = f.server();
    let mut client = Client::connect(restarted.descriptor());
    client.hello();
    assert_eq!(
        client.request("draft.apply", edit("slow", 1, "accepted"))["result"]["replayed"],
        true
    );
    drop(client);
    restarted.close().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_old_close_preserves_replacement_descriptor_and_idle_clients_do_not_wedge() {
    let f = Fixture::new();
    let mut old = f.server();
    let previous = old.descriptor().clone();
    let _idle = Client::connect(&previous);
    let mut replacement = f.server();
    let next = replacement.descriptor().clone();
    assert_ne!(next.token, previous.token);
    old.close().unwrap();
    let saved: Value =
        serde_json::from_slice(&fs::read(f.root.join("agent-data-connection.json")).unwrap())
            .unwrap();
    assert_eq!(saved["token"], next.token);
    assert_eq!(saved["endpoint"], next.endpoint);
    let mut client = Client::connect(&next);
    assert_eq!(client.hello()["ok"], true);
    drop(client);
    replacement.close().unwrap();
    assert!(!f.root.join("agent-data-connection.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_fragmented_utf8_lines_preserve_large_user_fields_without_payload_caps() {
    let f = Fixture::new();
    f.draft().await;
    let mut server = f.server();
    let mut client = Client::connect(server.descriptor());
    assert_eq!(client.hello()["ok"], true);
    let remark = "中文😀 % #\n".repeat(8192);
    let mut frame = serde_json::to_vec(&json!({"id":77,"token":server.descriptor().token,
        "method":"draft.apply","params":edit("fragmented",1,&remark)}))
    .unwrap();
    frame.push(b'\n');
    for bytes in frame.chunks(4093) {
        client.stream.get_mut().write_all(bytes).unwrap();
    }
    assert_eq!(client.reply()["result"]["replayed"], false);
    assert_eq!(
        client.request("draft.get", json!({"draftId":"d"}))["result"]["storyboard"][0]["remark"],
        remark
    );
    drop(client);
    server.close().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_shared_admission_stops_unpublished_and_published_listeners() {
    let f = Fixture::new();
    let stopping = Arc::new(AtomicBool::new(true));
    let mut unpublished = Server::start_with_admission(
        Options {
            data_root: f.root.clone(),
            temporary_directory: std::env::temp_dir(),
        },
        f.host.clone(),
        tokio::runtime::Handle::current(),
        stopping,
    )
    .unwrap();
    unpublished.close().unwrap();
    assert_eq!(f.host.calls.load(Ordering::SeqCst), 0);
    assert!(!f.root.join("agent-data-connection.json").exists());

    let stopping = Arc::new(AtomicBool::new(false));
    let mut published = Server::start_with_admission(
        Options {
            data_root: f.root.clone(),
            temporary_directory: std::env::temp_dir(),
        },
        f.host.clone(),
        tokio::runtime::Handle::current(),
        stopping.clone(),
    )
    .unwrap();
    let mut client = Client::connect(published.descriptor());
    assert_eq!(client.hello()["ok"], true);
    // The application can stop admission without acquiring the server slot.
    stopping.store(true, Ordering::Release);
    let mut line = String::new();
    assert_eq!(client.stream.read_line(&mut line).unwrap(), 0);
    assert_eq!(f.host.calls.load(Ordering::SeqCst), 0);
    published.close().unwrap();
}

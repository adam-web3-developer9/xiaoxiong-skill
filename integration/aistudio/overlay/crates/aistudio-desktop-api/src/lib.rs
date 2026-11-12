//! Public desktop IPC over native authorities. No Node transport, SQL escape
//! hatch, database writer, scheduler, or renderer-supplied principal is created.
mod public_media;
pub use public_media::PublicMediaHost;
mod data;
mod agent_channels;
mod knowledge;
mod library;
mod library_export;
mod local;
mod masking;
mod media;
mod media_export;
mod media_auxiliary;
mod folder_rename;
mod jianying;
mod zhihu;
mod prompt_export;
pub mod network_proxy;
pub use network_proxy::NetworkProxyHost;
pub use folder_rename::DirectoryHost;
pub use media_auxiliary::MediaProbeHost;
pub mod portable;
pub use aistudio_project_archive::project_zip;
pub mod project;
pub use project::ProjectHost;
mod updates;
pub use updates::{UpdateDownloadHost, UpdateInstallHost};
mod plugins;
mod channel_plugins;
mod joojo_login;
pub use joojo_login::JoojoLoginHost;
pub use plugins::PluginHost;
mod prompts;
pub mod read;
mod settings_transfer;
mod search;
pub use search::SearchHost;
mod task_controls;
mod task_preparation;
mod task_recovery;
pub use task_recovery::TaskRecoveryHost;
mod package_sharing;
mod task_manual_relay;
mod legacy_sharing;
mod task_chatgpt_relay;
mod intent;
mod tasks;
pub use local::LocalDesktopHost;
pub use media::NativeUploads;
pub use media_export::{ExportRoute, MediaExportHost, MediaIoLease};
pub use settings_transfer::TransferHost;
mod effects;
use aistudio_account::AccountService;
use aistudio_runtime_backend::RuntimeBackend;
pub use effects::{DesktopEffects, EffectFuture, SaveDialog};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Error {
    pub name: String,
    pub code: String,
    pub message: String,
    pub details: Value,
    #[serde(flatten)]
    pub properties: serde_json::Map<String, Value>,
    #[serde(skip)]
    pub committed_events: Vec<Value>,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
impl From<aistudio_project_archive::Error> for Error {
    fn from(e: aistudio_project_archive::Error) -> Self {
        Self { name:e.name, code:e.code, message:e.message, details:e.details, properties:e.properties, committed_events:Vec::new() }
    }
}
impl From<aistudio_runtime_backend::Error> for Error {
    fn from(e: aistudio_runtime_backend::Error) -> Self {
        Self {
            name: "StorageError".into(),
            code: e.code,
            message: e.message,
            details: e.details,
            properties: serde_json::Map::new(),
            committed_events: vec![],
        }
    }
}
impl From<aistudio_account::Error> for Error {
    fn from(e: aistudio_account::Error) -> Self {
        let mut details = json!({});
        if let Some(status) = e.status {
            details["status"] = json!(status);
        }
        if let Some(payload) = e.payload {
            details["payload"] = payload;
        }
        details["taskError"] = e.task_error;
        Self {
            name: "Error".into(),
            code: e.code,
            message: e.message,
            details: json!({}),
            properties: details.as_object().unwrap().clone(),
            committed_events: vec![],
        }
    }
}
impl From<aistudio_config_domain::Error> for Error {
    fn from(e: aistudio_config_domain::Error) -> Self {
        Self {
            name: "Error".into(),
            code: e.code,
            message: e.message,
            details: e.details,
            properties: serde_json::Map::new(),
            committed_events: vec![],
        }
    }
}
pub fn error(code: &str, message: impl Into<String>) -> Error {
    Error {
        name: "Error".into(),
        code: code.into(),
        message: message.into(),
        details: json!({}),
        properties: serde_json::Map::new(),
        committed_events: vec![],
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Reply {
    pub value: Value,
    pub events: Vec<Value>,
}
impl Reply {
    fn value(value: Value) -> Self {
        Self {
            value,
            events: vec![],
        }
    }
}
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

pub struct DesktopApi {
    pub(crate) task_recovery: Option<Arc<dyn TaskRecoveryHost>>,
    pub(crate) task_preparation: Option<Arc<aistudio_task_preparation::generic_owner::TaskPreparationService>>,
    pub(crate) package_sharing: Option<Arc<aistudio_package_sharing::cloud::PackageService>>,
    pub(crate) public_media: Option<Arc<dyn PublicMediaHost>>,
    pub(crate) chatgpt_relay: Option<Arc<aistudio_chatgpt_relay::RelayService>>,
    pub(crate) manual_relay: Option<Arc<aistudio_manual_relay::ManualRelayService>>,
    pub(crate) root: PathBuf,
    pub(crate) backend: Arc<RuntimeBackend>,
    pub(crate) account: Arc<AccountService>,
    pub(crate) effects: Arc<dyn DesktopEffects>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) application: Option<Arc<aistudio_task_host::NativeApplication>>,
    pub(crate) uploads: Option<Arc<dyn NativeUploads>>,
    pub(crate) local_host: Option<Arc<dyn LocalDesktopHost>>,
    pub(crate) media_export_host: Option<Arc<dyn MediaExportHost>>,
    pub(crate) media_export_io: media_export::MediaIoOwner,
    pub(crate) media_probe_host: Option<Arc<dyn MediaProbeHost>>,
    pub(crate) directory_host: Option<Arc<dyn DirectoryHost>>,
    pub(crate) folder_tools: folder_rename::FolderTools,
    pub(crate) network_proxy_host: Option<Arc<dyn NetworkProxyHost>>,
    pub(crate) transfer_host: Option<Arc<dyn TransferHost>>,
    pub(crate) pending_settings: settings_transfer::Sessions,
    pub(crate) search_host: Option<Arc<dyn SearchHost>>,
    pub(crate) updates: Option<Arc<aistudio_updates::Controller>>,
    pub(crate) update_install_host: Option<Arc<dyn UpdateInstallHost>>,
    pub(crate) update_download_host: Option<Arc<dyn UpdateDownloadHost>>,
    pub(crate) plugin_host: Option<Arc<dyn PluginHost>>,
    pub(crate) project_host: Option<Arc<dyn ProjectHost>>,
    pub(crate) joojo_login_host: Option<Arc<dyn JoojoLoginHost>>,
    pub(crate) project_state: std::sync::Mutex<project::State>,
    pub(crate) search_failed_at: std::sync::Mutex<Option<i64>>,
}
impl DesktopApi {
    /// Authorities are the application's existing instances. Constructing this
    /// facade performs no I/O and does not transfer or duplicate ownership.
    pub fn new(
        root: PathBuf,
        backend: Arc<RuntimeBackend>,
        account: Arc<AccountService>,
        effects: Arc<dyn DesktopEffects>,
    ) -> Self {
        let authority = account.clone();
        backend.bind_principal_source(Arc::new(move || authority.snapshot()));
        Self {
            root,
            backend,
            account,
            effects,
            clock: Arc::new(SystemClock),
            application: None,
            uploads: None,
            local_host: None,
            media_export_host: None,
            media_export_io: Default::default(),
            media_probe_host: None,
            directory_host: None,
            folder_tools: Default::default(),
            network_proxy_host: None,
            transfer_host: None,
            pending_settings: Default::default(),
            task_preparation: None,
            task_recovery: None,
            package_sharing: None,
            manual_relay: None,
            chatgpt_relay: None,
            public_media: None,
            search_host: None,
            updates: None,
            update_install_host: None,
            update_download_host: None,
            plugin_host: None,
            project_host: None,
            joojo_login_host: None,
            project_state: Default::default(),
            search_failed_at: Default::default(),
        }
    }
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
    pub fn with_update_download_host(mut self, host: Arc<dyn UpdateDownloadHost>) -> Self {
        self.update_download_host = Some(host);
        self
    }
    pub fn with_application(
        mut self,
        application: Arc<aistudio_task_host::NativeApplication>,
    ) -> Self {
        self.application = Some(application);
        self
    }
    pub fn with_uploads(mut self, uploads: Arc<dyn NativeUploads>) -> Self {
        self.uploads = Some(uploads);
        self
    }
    pub fn with_local_host(mut self, host: Arc<dyn LocalDesktopHost>) -> Self {
        self.local_host = Some(host);
        self
    }
    pub fn with_media_export_host(mut self, host: Arc<dyn MediaExportHost>) -> Self {
        self.media_export_host = Some(host);
        self
    }
    pub fn with_media_probe_host(mut self, host: Arc<dyn MediaProbeHost>) -> Self {
        self.media_probe_host = Some(host);
        self
    }
    pub fn with_directory_host(mut self, host: Arc<dyn DirectoryHost>) -> Self {
        self.directory_host = Some(host);
        self
    }
    pub fn with_network_proxy_host(mut self, host: Arc<dyn NetworkProxyHost>) -> Self {
        self.network_proxy_host = Some(host);
        self
    }
    pub fn with_transfer_host(mut self, host: Arc<dyn TransferHost>) -> Self {
        self.transfer_host = Some(host);
        self
    }
    pub fn with_search_host(mut self, host: Arc<dyn SearchHost>) -> Self {
        self.search_host = Some(host);
        self
    }
    pub fn with_project_host(mut self, host: Arc<dyn ProjectHost>) -> Self {
        self.project_host = Some(host);
        self
    }
    pub fn with_plugin_host(mut self, host: Arc<dyn PluginHost>) -> Self {
        self.plugin_host = Some(host);
        self
    }
    pub fn with_joojo_login_host(mut self, host: Arc<dyn JoojoLoginHost>) -> Self {
        self.joojo_login_host = Some(host);
        self
    }
    pub fn with_updates(mut self, controller: Arc<aistudio_updates::Controller>, host: Arc<dyn UpdateInstallHost>) -> Self {
        self.updates = Some(controller);
        self.update_install_host = Some(host);
        self
    }
    pub fn with_manual_relay(mut self, relay: Arc<aistudio_manual_relay::ManualRelayService>) -> Self {
        self.manual_relay = Some(relay);
        self
    }
    pub fn with_chatgpt_relay(mut self, relay: Arc<aistudio_chatgpt_relay::RelayService>) -> Self {
        self.chatgpt_relay = Some(relay);
        self
    }
    pub fn with_public_media(mut self,host:Arc<dyn PublicMediaHost>)->Self{self.public_media=Some(host);self}
    pub fn with_task_recovery(mut self,host:Arc<dyn TaskRecoveryHost>)->Self{self.task_recovery=Some(host);self}
    pub fn with_task_preparation(mut self,service:Arc<aistudio_task_preparation::generic_owner::TaskPreparationService>)->Self{self.task_preparation=Some(service);self}
    pub fn with_package_sharing(mut self,service:Arc<aistudio_package_sharing::cloud::PackageService>)->Self{self.package_sharing=Some(service);self}
    pub fn registered(channel: &str) -> bool {
        contract().iter().any(|v| v["channel"] == channel)
    }
    pub fn supports(channel: &str) -> bool {
        read::supports(channel)
            || data::supports(channel)
            || effects::supports(channel)
            || prompts::supports(channel)
            || tasks::supports(channel)
            || task_preparation::supports(channel)
            || task_recovery::supports(channel)
            || package_sharing::supports(channel)
            || task_controls::supports(channel)
            || task_manual_relay::supports(channel)
            || legacy_sharing::supports(channel)
            || task_chatgpt_relay::supports(channel)
            || public_media::supports(channel)
            || intent::supports(channel)
            || media::supports(channel)
            || library::supports(channel)
            || local::supports(channel)
            || media_export::supports(channel)
            || library_export::supports(channel)
            || settings_transfer::supports(channel)
            || search::supports(channel)
            || media_auxiliary::supports(channel)
            || folder_rename::supports(channel)
            || jianying::supports(channel)
            || zhihu::supports(channel)
            || prompt_export::supports(channel)
            || network_proxy::supports(channel)
            || project::supports(channel)
            || plugins::supports(channel)
            || channel_plugins::supports(channel)
            || joojo_login::supports(channel)
            || updates::supports(channel)
    }
    /// Public response shapes match the existing renderer contract. Account
    /// authorization precedes all reads, dialog effects, and business writes.
    /// Undefined object fields are a transport sideband relative to args (e.g.
    /// /0/commands/0/fields/remark), never markers inserted in user JSON.
    pub async fn invoke(&self, channel: &str, args: &[Value], undefined_paths: &[String]) -> Reply {
        let result = self.invoke_result(channel, args, undefined_paths).await;
        match result {
            Ok(value) => value,
            Err(mut failure) => {
                if channel == "data:apply" && failure.name == "StorageError" {
                    failure.name = "DataCommandError".into();
                }
                let mut detail = json!({"name":failure.name,"message":failure.message,"code":failure.code,"details":failure.details});
                if failure.code.is_empty() {
                    detail.as_object_mut().unwrap().shift_remove("code");
                }
                if channel != "data:apply"
                    && (failure.name == "Error" || (channel == "prompt:preview" && failure.name == "VideoPromptLengthError"))
                    && failure
                        .details
                        .as_object()
                        .is_some_and(|fields| fields.is_empty())
                {
                    detail.as_object_mut().unwrap().shift_remove("details");
                }
                for key in [
                    "status",
                    "payload",
                    "missing",
                    "backgroundResponse",
                    "retryable",
                    "run",
                    "providerTerminal",
                    "providerRefunded",
                    "submitOutcome",
                    "taskError",
                ] {
                    if let Some(value) = failure
                        .properties
                        .get(key)
                        .or_else(|| failure.details.get(key))
                    {
                        detail[key] = value.clone();
                    }
                }
                let detail =
                    aistudio_runtime_backend::redaction::Redactor::default().diagnostic(&detail);
                Reply {
                    value: if channel == "data:apply" {
                        json!({"ok":false,"error":detail})
                    } else {
                        json!({"__aistudioIpcError":1,"error":detail})
                    },
                    events: failure.committed_events,
                }
            }
        }
    }
    pub async fn invoke_result(
        &self,
        channel: &str,
        args: &[Value],
        undefined_paths: &[String],
    ) -> Result<Reply> {
        if !Self::registered(channel) {
            return Err(error(
                "DESKTOP_CHANNEL_UNKNOWN",
                format!("Unknown public desktop channel: {channel}"),
            ));
        }
        if matches!(channel, "update:prepare-install" | "update:release-install" | "update:drain-install" | "update:install-status") {
            return self.updates(channel,args).await;
        }
        if !shell_channel(channel) {
            self.account
                .assert_local()
                .map_err(|failure| error(&failure.code, failure.message))?;
        }
        if read::supports(channel) {
            return read::read(&self.root, channel, args)
                .map(Reply::value)
                .map_err(|message| {
                    let (code, text) = message
                        .split_once(": ")
                        .unwrap_or(("NATIVE_DATA_READ_FAILED", &message));
                    error(code, text)
                });
        }
        if search::supports(channel) {
            return self.search(args);
        }
        if project::supports(channel) {
            return self.project(channel, args).await;
        }
        if effects::supports(channel) {
            return self.effect(channel, args).await;
        }
        if updates::supports(channel) {
            return self.updates(channel, args).await;
        }
        if joojo_login::supports(channel) { return self.joojo_login(channel, args).await; }
        if channel_plugins::supports(channel) { return self.channel_plugin(channel, args).await; }
        if plugins::supports(channel) {
            return self.plugin(channel, args).await;
        }
        if prompts::supports(channel) {
            return self.prompt(channel, args, undefined_paths);
        }
        if public_media::supports(channel){return Box::pin(self.public_media(channel,args)).await;}
        if task_chatgpt_relay::supports(channel) {
            return self.task_chatgpt_relay(channel,args).await;
        }
        if legacy_sharing::supports(channel) {
            return self.legacy_sharing(channel);
        }
        if task_manual_relay::supports(channel) {
            return self.task_manual_relay(channel,args);
        }
        if intent::supports(channel) {
            if let Some(service) = &self.task_preparation {
                let principal=self.account.snapshot();
                let commit=service.execute_public(arg(args,0),Some(&principal)).await.map_err(Error::from)?;
                return Ok(Reply::value(commit.value));
            }
            return self.intent(args);
        }
        if task_recovery::supports(channel) { return self.task_recovery(channel,args).await; }
        if task_preparation::supports(channel) { return self.task_preparation(channel,args).await; }
        if package_sharing::supports(channel) { return self.package_sharing(channel,args).await; }
        if task_controls::supports(channel) {
            return self.task_control(channel,args).await;
        }
        if tasks::supports(channel) {
            return self.task(channel, args).await;
        }
        if media::supports(channel) {
            return self.media(channel, args);
        }
        if library::supports(channel) {
            return self.library(channel, args);
        }
        if local::supports(channel) {
            return self.local(channel, args).await;
        }
        if settings_transfer::supports(channel) {
            return self.settings_transfer(channel, args).await;
        }
        if media_export::supports(channel) {
            return self.media_export(channel, args).await;
        }
        if media_auxiliary::supports(channel) {
            return self.media_auxiliary(channel, args).await;
        }
        if folder_rename::supports(channel) {
            return self.folder_rename(channel, args).await;
        }
        if jianying::supports(channel) {
            return self.jianying(channel, args).await;
        }
        if zhihu::supports(channel) {
            return self.zhihu(channel, args).await;
        }
        if prompt_export::supports(channel) {
            return self.prompt_export(channel, args).await;
        }
        if network_proxy::supports(channel) {
            return self.network_proxy(channel, args).await;
        }
        if library_export::supports(channel) {
            return self.library_export(channel, args).await;
        }
        self.data(channel, args, undefined_paths).await
    }
    pub(crate) fn runtime(
        &self,
        op: &str,
        args: Value,
        undefined_paths: &[String],
    ) -> Result<Reply> {
        let commit = self
            .backend
            .call_as(
                self.account.snapshot(),
                op,
                args,
                undefined_paths,
                self.clock.now_ms(),
            )
            .map_err(|failure| {
                let mut failure = Error::from(failure);
                failure.committed_events = self.backend.take_committed_v4_events();
                failure
            })?;
        Ok(Reply {
            value: commit.value,
            events: commit.events,
        })
    }
    pub(crate) fn v4(&self, op: &str, args: Value, undefined_paths: &[String]) -> Result<Reply> {
        let commit =
            self.backend
                .dispatch_v4_authenticated(op, args, undefined_paths)?;
        Ok(Reply {
            value: commit.value,
            events: commit.events,
        })
    }
}
fn contract() -> &'static Vec<Value> {
    static CONTRACT: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(include_str!("../channels.json")).expect("generated IPC contract")
    })
}
fn shell_channel(channel: &str) -> bool {
    matches!(
        channel,
        "account:status"
            | "account:action"
            | "host:current-epoch"
            | "host:build-info"
            | "host:verify-build"
            | "open-external-url"
            | "app-update-check"
            | "app-update-download"
            | "app-update-cancel"
            | "app-update-channel"
            | "app-update-get-status"
            | "app-update-install"
    )
}
pub(crate) fn arg(args: &[Value], index: usize) -> &Value {
    args.get(index).unwrap_or(&Value::Null)
}
pub(crate) fn string_arg(args: &[Value], index: usize) -> String {
    args.get(index)
        .map(aistudio_config_domain::string)
        .unwrap_or("undefined".into())
}


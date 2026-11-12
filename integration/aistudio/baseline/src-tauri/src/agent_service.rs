//! Agent transport borrows the application's one DesktopService. Construction
//! performs no IO; start() is called only after the legacy Agent server retires.
use aistudio_agent_data::{Error, Host, HostFuture, Operation, Options, Result, Server};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::Manager;

/// The application's shared native diagnostic owner applies its known-secret
/// redactor and context, then preserves the existing operational destination:
/// <profile>/diagnostics/operational-diagnostics.jsonl, 5 MiB and ten files.
/// Agent never creates another file writer or invents a runtime_events type.
pub type OperationSink = Arc<dyn Fn(&str, Value) + Send + Sync>;

struct AppHost {
    app: tauri::AppHandle,
    operation: OperationSink,
}
fn unavailable() -> Error {
    Error::new(
        "AGENT_COMMAND_UNAVAILABLE",
        "Application command is not ready",
    )
}
fn wire_error(value: Value) -> Error {
    let mut error = Error::new(
        value["code"]
            .as_str()
            .filter(|code| !code.is_empty())
            .unwrap_or("AGENT_COMMAND_FAILED"),
        value["message"]
            .as_str()
            .filter(|message| !message.is_empty())
            .unwrap_or("Application command failed"),
    );
    error.details = value.get("details").cloned().unwrap_or_else(|| json!({}));
    error
}
impl Host for AppHost {
    fn build_snapshot(&self, client_id: &str) -> Value {
        self.app
            .try_state::<crate::desktop_service::DesktopService>()
            .map(|desktop| desktop.builds.lock().unwrap().snapshot(client_id))
            .unwrap_or_else(|| json!({"main":null,"host":null,"renderer":null,"mismatched":false,"writable":false}))
    }
    fn verify_build(&self, client_id: &str, build_id: &str) -> Result<Value> {
        let desktop = self
            .app
            .try_state::<crate::desktop_service::DesktopService>()
            .ok_or_else(unavailable)?;
        // Session already compared main exactly before reaching this shared,
        // sticky build gate. Its generated agent ID never reuses main's label.
        Ok(desktop.verified(client_id, &json!(build_id)))
    }
    fn forget_build(&self, client_id: &str) {
        if let Some(desktop) = self
            .app
            .try_state::<crate::desktop_service::DesktopService>()
        {
            desktop.builds.lock().unwrap().forget(client_id);
        }
    }
    fn invoke<'a>(
        &'a self,
        channel: &'a str,
        client_id: &'a str,
        args: Vec<Value>,
    ) -> HostFuture<'a, Value> {
        Box::pin(async move {
            let desktop = self
                .app
                .try_state::<crate::desktop_service::DesktopService>()
                .ok_or_else(unavailable)?;
            desktop
                .guard_write(client_id, channel)
                .map_err(wire_error)?;
            // invoke publishes the committed events itself. Calling events()
            // here again would duplicate renderer and derived-state updates.
            desktop
                .invoke(channel.into(), args, vec![])
                .await
                .map_err(wire_error)
        })
    }
    fn operation(&self, record: Operation) {
        // Operation contains correlation only, never args, tokens or responses.
        // The shared sink additionally handles known credential literals.
        if let Ok(details) = serde_json::to_value(record) {
            (self.operation)("agent.command", details);
        }
    }
}

pub struct AgentService {
    app: tauri::AppHandle,
    data_root: PathBuf,
    temporary_directory: PathBuf,
    operation: OperationSink,
    owner: Arc<Mutex<Option<Server>>>,
    stopping: Arc<AtomicBool>,
    lifecycle: tokio::sync::Mutex<()>,
    close_state: tokio::sync::Mutex<CloseState>,
}
enum CloseState {
    Ready,
    Running(tauri::async_runtime::JoinHandle<Result<()>>),
    Finished(Result<()>),
}
impl AgentService {
    /// Ready but unstarted. The caller explicitly transfers Agent ownership
    /// after retiring the old Node server; new() never binds a socket or writes
    /// the descriptor and never creates an API, account, build gate or writer.
    pub fn new(
        app: tauri::AppHandle,
        data_root: PathBuf,
        temporary_directory: PathBuf,
        operation: OperationSink,
    ) -> Self {
        Self {
            app,
            data_root,
            temporary_directory,
            operation,
            owner: Arc::new(Mutex::new(None)),
            stopping: Arc::new(AtomicBool::new(false)),
            lifecycle: tokio::sync::Mutex::new(()),
            close_state: tokio::sync::Mutex::new(CloseState::Ready),
        }
    }
    pub async fn start(&self) -> Result<()> {
        let _lifecycle = self.lifecycle.lock().await;
        let owner = self.owner.clone();
        let stopping = self.stopping.clone();
        let options = Options {
            data_root: self.data_root.clone(),
            temporary_directory: self.temporary_directory.clone(),
        };
        let host = Arc::new(AppHost {
            app: self.app.clone(),
            operation: self.operation.clone(),
        });
        let runtime = tokio::runtime::Handle::current();
        tauri::async_runtime::spawn_blocking(move || {
            let mut slot = owner.lock().unwrap();
            if stopping.load(Ordering::Acquire) {
                return Err(Error::new("AGENT_SERVICE_CLOSED", "Agent 数据工具已关闭"));
            }
            if slot.is_some() {
                return Ok(());
            }
            let mut server = Server::start_with_admission(options, host, runtime, stopping.clone())
                .map_err(|error| Error::new("AGENT_TRANSPORT_FAILED", error.to_string()))?;
            // stop_admission may race an in-flight filesystem start. The
            // single slot remains owned until that new listener is drained.
            if stopping.load(Ordering::Acquire) {
                server
                    .close()
                    .map_err(|error| Error::new("AGENT_TRANSPORT_FAILED", error.to_string()))?;
                return Err(Error::new("AGENT_SERVICE_CLOSED", "Agent 数据工具已关闭"));
            } else {
                *slot = Some(server);
            }
            Ok(())
        })
        .await
        .map_err(|error| Error::new("AGENT_WORKER_FAILED", error.to_string()))?
    }
    pub fn stop_admission(&self) {
        // The accept loop shares this flag even before start publishes its
        // server slot; stopping never waits for filesystem IO on this thread.
        self.stopping.store(true, Ordering::Release);
    }
    pub async fn close(&self) -> Result<()> {
        self.stop_admission();
        let _lifecycle = self.lifecycle.lock().await;
        let mut close = self.close_state.lock().await;
        if matches!(*close, CloseState::Ready) {
            let owner = self.owner.clone();
            *close = CloseState::Running(tauri::async_runtime::spawn_blocking(move || {
                let server = owner.lock().unwrap().take();
                if let Some(mut server) = server {
                    server
                        .close()
                        .map_err(|error| Error::new("AGENT_TRANSPORT_FAILED", error.to_string()))?;
                }
                Ok(())
            }));
        }
        let result = match &mut *close {
            // Keep the join handle in this owner while awaiting it. Cancelling
            // a timed-out close must not let its retry report success before
            // the original accepted operation has actually drained.
            CloseState::Running(worker) => worker
                .await
                .map_err(|error| Error::new("AGENT_WORKER_FAILED", error.to_string()))
                .and_then(|result| result),
            CloseState::Finished(result) => return result.clone(),
            CloseState::Ready => unreachable!(),
        };
        *close = CloseState::Finished(result.clone());
        result
    }
}
impl Drop for AgentService {
    fn drop(&mut self) {
        self.stop_admission();
    }
}

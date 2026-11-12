use aistudio_agent_data::Error;
use serde_json::{json, Value};
use tauri::Manager;

/// Dedicated IPC, deliberately absent from the socket method registry.
#[tauri::command]
pub fn agent_authorization(
    window: tauri::WebviewWindow,
    action: String,
    request_id: Option<String>,
    session_id: Option<String>,
) -> std::result::Result<Value, Value> {
    if !crate::bridge::trusted_window(&window) {
        return Err(json!({"code":"DESKTOP_ORIGIN_DENIED","message":"此页面不能管理 Agent 授权"}));
    }
    let app = window.app_handle();
    let desktop = app
        .try_state::<crate::desktop_service::DesktopService>()
        .ok_or_else(|| json!({"code":"HOST_NOT_READY"}))?;
    desktop.guard_write(window.label(), "agent:authorization")?;
    let service = app
        .try_state::<crate::agent_service::AgentService>()
        .ok_or_else(|| json!({"code":"HOST_NOT_READY"}))?;
    let auth = service
        .authorization_authority()
        .ok_or_else(|| json!({"code":"HOST_NOT_READY"}))?;
    let context = desktop.account.agent_authorization_context();
    let result = match action.as_str() {
        "list" => Ok(()),
        "approve" | "deny" => auth.decide(
            request_id.as_deref().unwrap_or(""),
            action == "approve",
            &context,
        ),
        "revoke" => {
            auth.revoke(session_id.as_deref().unwrap_or(""));
            Ok(())
        }
        _ => Err(Error::new(
            "AGENT_METHOD_NOT_ALLOWED",
            "Unknown authorization action",
        )),
    };
    result.map_err(|e| serde_json::to_value(e).unwrap())?;
    let mut state = auth.list(&context);
    state["legacyTrustedDevelopment"] =
        json!(std::env::var("AISTUDIO_AGENT_LEGACY_V1").as_deref() == Ok("1"));
    Ok(state)
}

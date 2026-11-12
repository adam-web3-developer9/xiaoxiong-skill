//! Typed external-client channel configuration. Internal secrets stay in this
//! owner; no caller-supplied config, references, destination or schema is used
//! for dynamic plugin calls.
use crate::*;
fn safe_error(code: &str) -> Error {
    let code = if !code.is_empty()
        && code
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
    {
        code
    } else {
        "CHANNEL_OPERATION_FAILED"
    };
    error(code, "渠道操作失败，请在应用内查看详情")
}
mod public;
use public::*;
impl DesktopApi {
    pub(crate) async fn agent_channels(&self, channel: &str, args: &Value) -> Result<Reply> {
        let host = self
            .plugin_host
            .as_ref()
            .ok_or_else(|| safe_error("PLUGIN_HOST_NOT_READY"))?;
        let service = host
            .channels()
            .ok_or_else(|| safe_error("CHANNEL_PLUGIN_HOST_MISSING"))?;
        let manifests = service.list().await.map_err(|e| safe_error(&e.code))?;
        let mut input = aistudio_config_domain::object(args);
        input["manifests"] = manifests;
        if channel != "local-data:channel-config-options" {
            return self
                .runtime(
                    if channel == "local-data:channel-config-list" {
                        "config.channels.list"
                    } else {
                        "config.channels.patch"
                    },
                    input,
                    &[],
                )
                .map_err(|e| safe_error(&e.code));
        }
        // The config owner reads the provider and its credentials in one native
        // transaction, so endpoint and secret cannot come from different saves.
        let context = self
            .runtime("config.channels.optionsContext", input, &[])
            .map_err(|e| safe_error(&e.code))?
            .value;
        let capability = args["capability"].as_str().unwrap_or("");
        let binding = service
            .binding(context["pluginId"].as_str().unwrap_or(""), capability)
            .await
            .map_err(|e| safe_error(&e.code))?;
        if binding["version"] != context["manifest"]["version"]
            || binding["packageDigest"] != context["manifest"]["packageDigest"]
        {
            return Err(safe_error("PLUGIN_BINDING_MISMATCH"));
        }
        let config = context["config"].clone();
        let mut secrets = Vec::new();
        let mut private_names = Vec::new();
        secret_names(&context["manifest"]["configSchema"], &mut private_names);
        for key in ["apiKey", "secondaryApiKey"] {
            collect_strings(&config[key], &mut secrets);
        }
        sensitive_values(&config, &context["manifest"]["configSchema"], &mut secrets);
        for secret in secrets.clone() {
            let encoded = encoded_secret(&secret);
            secrets.push(encoded.clone());
            secrets.push(
                encoded
                    .trim_end_matches('=')
                    .replace('+', "-")
                    .replace('/', "_"),
            );
            secrets.push(
                secret
                    .as_bytes()
                    .iter()
                    .map(|b| {
                        if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
                            (*b as char).to_string()
                        } else {
                            format!("%{b:02X}")
                        }
                    })
                    .collect(),
            );
        }
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        secrets.dedup();
        let request = aistudio_channel_plugin::protocol_wire::Request {
            call_id: uuid::Uuid::new_v4().to_string(),
            execution_key: String::new(),
            explicit_retry_key: None,
            operation: "configurationOptions".into(),
            capability: capability.into(),
            input: json!({"action":args["action"]}),
            config,
            transport: json!({"generation":{"route":{"kind":"hostResolved"}}}),
            checkpoint: json!({}),
            media_grants: json!({}),
        };
        let raw = host
            .channel_configuration_options(&binding, request)
            .await
            .map_err(|e| safe_error(&e.code))?;
        // All current UI protocol sections survive, including provider-defined
        // columns/details/updates/features/stats/selection/parameters. No model
        // names, plugin IDs, endpoint catalogs or option actions are hardcoded.
        let mut result = json!({});
        for field in [
            "options",
            "columns",
            "description",
            "parameters",
            "selection",
            "status",
        ] {
            if let Some(value) = raw.get(field) {
                result[field] = public_result(value, &secrets, &private_names);
            }
        }
        Ok(Reply::value(result))
    }
}

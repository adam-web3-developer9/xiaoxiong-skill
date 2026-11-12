//! Agent configuration commands execute inside the existing RuntimeStore
//! transaction. The desktop owner supplies installed manifests, never callers.
use super::*;
use aistudio_config_domain::{
    channel_capability_config, collect_secret_writes, secret_presence_view, strip_secret_inputs,
};

const HOST_FIELDS: &[&str] = &[
    "id",
    "name",
    "enabled",
    "kind",
    "type",
    "provider",
    "baseUrl",
    "model",
    "textModel",
    "imageModel",
    "videoModel",
    "voiceModel",
    "imageModelId",
    "videoModelId",
    "channelPluginId",
    "channelPluginType",
    "channelPluginVersion",
    "channelPluginBindings",
    "channelPluginCapabilities",
    "capabilities",
    "videoResolution",
    "videoAspectRatio",
    "videoDuration",
    "videoCopies",
    "videoTaskConcurrency",
    "videoPollIntervalSeconds",
    "videoPollTimeoutMinutes",
    "imageSize",
    "imageQuality",
    "imageAspectRatio",
    "imageCount",
    "imageMode",
    "textTemperature",
    "textMaxTokens",
    "timeoutMs",
    "maxConcurrent",
    "textTaskConcurrency",
    "imageTaskConcurrency",
    "inheritCredentials",
];
const SECRET_FIELDS: [(&str, &str); 2] = [
    ("apiKey", "secretRef"),
    ("secondaryApiKey", "secondarySecretRef"),
];
fn invalid() -> Error {
    error(
        "CHANNEL_CONFIG_INVALID",
        "渠道设置字段不符合当前插件配置协议",
    )
}
fn plugin_id<'a>(provider: &'a Value, capability: &str) -> &'a str {
    provider["channelPluginBindings"][capability]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| provider["channelPluginId"].as_str())
        .unwrap_or("")
}
fn schema(provider: &Value, capability: &str, manifests: &Value) -> Value {
    let id = plugin_id(provider, capability);
    manifests
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["id"] == id)
        .map(|m| m["configSchema"].clone())
        .unwrap_or_else(|| json!({"type":"object","properties":{}}))
}
fn secret_key(key: &str) -> bool {
    SECRET_FIELDS
        .iter()
        .any(|(field, reference)| key == *field || key == *reference)
        || [
            "apiKeyRef",
            "channelPluginResolvedCapability",
            "channelPluginConfigSource",
        ]
        .contains(&key)
        || key.ends_with("Configured")
        || key.ends_with("Fingerprint")
        || key.ends_with("Clear")
}
fn private_field(schema: &Value, key: &str) -> bool {
    secret_key(key)
        || schema["properties"][key]["secret"] == true
        || schema["properties"][key]["writeOnly"] == true
        || schema["properties"].as_object().is_some_and(|fields| {
            fields.values().any(|field| {
                (field["secret"] == true || field["writeOnly"] == true)
                    && field["legacyField"] == key
            })
        })
}
fn schema_public(value: &Value, field: &Value) -> Option<Value> {
    if field["secret"] == true || field["writeOnly"] == true {
        return None;
    }
    match value {
        Value::Object(values) => {
            let mut out = serde_json::Map::new();
            for (key, value) in values {
                if private_field(field, key) {
                    continue;
                }
                if let Some(declared) = field["properties"].get(key) {
                    if let Some(value) = schema_public(value, declared) {
                        out.insert(key.clone(), value);
                    }
                }
            }
            Some(Value::Object(out))
        }
        Value::Array(values) => Some(Value::Array(
            values
                .iter()
                .filter_map(|v| schema_public(v, &field["items"]))
                .collect(),
        )),
        _ => Some(value.clone()),
    }
}
fn public_provider(provider: &Value, manifests: &Value) -> Value {
    fn scope(value: &Value, schema: &Value) -> Value {
        let mut out = schema_public(value, schema).unwrap_or_else(|| json!({}));
        for key in HOST_FIELDS {
            if private_field(schema, key) {
                continue;
            }
            if let Some(value) = value.get(*key) {
                // Host metadata accepts scalar/string-list fields and plugin bindings.
                // Arbitrary nested blobs are not a public channel metadata protocol.
                if *key == "baseUrl" && value.as_str().is_some_and(|s| s.contains(['@', '?', '#']))
                {
                    out.as_object_mut().unwrap().shift_remove(*key);
                    continue;
                }
                if schema["properties"].get(*key).is_some() {
                    continue;
                }
                if value.is_array() && !value.as_array().unwrap().iter().all(Value::is_string) {
                    continue;
                }
                if value.is_object() && *key != "channelPluginBindings" && *key != "capabilities" {
                    continue;
                }
                if value.is_object()
                    && !value.as_object().unwrap().values().all(|v| {
                        if *key == "capabilities" {
                            v.is_boolean()
                        } else {
                            v.is_string()
                        }
                    })
                {
                    continue;
                }
                if *key == "baseUrl" && value.as_str().is_some_and(|s| s.contains(['@', '?', '#']))
                {
                    out.as_object_mut().unwrap().shift_remove(*key);
                    continue;
                }
                out[*key] = value.clone();
            }
        }
        let mut configured = json!({"primary":value["apiKeyConfigured"] == true,"secondary":value["secondaryApiKeyConfigured"] == true});
        fn declared_status(value: &Value, schema: &Value, path: &str, statuses: &mut Value) {
            if schema["secret"] == true || schema["writeOnly"] == true {
                statuses[path] = json!(!is_cleared(value));
            } else if let Some(fields) = schema["properties"].as_object() {
                for (key, field) in fields {
                    declared_status(&value[key], field, &format!("{path}/{key}"), statuses);
                }
            } else if let Some(values) = value.as_array() {
                for (i, value) in values.iter().enumerate() {
                    declared_status(value, &schema["items"], &format!("{path}/{i}"), statuses);
                }
            }
        }
        let mut plugin_status = json!({});
        declared_status(value, schema, "", &mut plugin_status);
        configured["plugin"] = plugin_status;
        out["credentialsConfigured"] = configured;
        out
    }
    let mut result = scope(provider, &schema(provider, "", manifests));
    if let Some(configs) = provider["channelPluginCapabilityConfigs"].as_object() {
        result["channelPluginCapabilityConfigs"] = Value::Object(
            configs
                .iter()
                .map(|(cap, value)| (cap.clone(), scope(value, &schema(provider, cap, manifests))))
                .collect(),
        );
    }
    result
}
fn validate_type(value: &Value, field: &Value) -> Result<()> {
    let valid = match field["type"].as_str() {
        Some("string") => value.is_string(),
        Some("number") => value.is_number(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("boolean") => value.is_boolean(),
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("null") => value.is_null(),
        _ => true,
    };
    if !valid
        || field["enum"]
            .as_array()
            .is_some_and(|items| !items.contains(value))
    {
        return Err(invalid());
    }
    Ok(())
}
fn has_private_schema(field: &Value) -> bool {
    field["secret"] == true
        || field["writeOnly"] == true
        || field["properties"]
            .as_object()
            .is_some_and(|fields| fields.values().any(has_private_schema))
        || field.get("items").is_some_and(has_private_schema)
}
fn patch_public(saved: &mut Value, patch: &Value, fields: &Value, host: bool) -> Result<()> {
    for (key, value) in patch.as_object().ok_or_else(invalid)? {
        let field = &fields["properties"][key];
        if key == "id" || private_field(fields, key) {
            return Err(invalid());
        }
        if field.is_null() && !(host && HOST_FIELDS.contains(&key.as_str())) {
            return Err(invalid());
        }
        validate_type(value, field)?;
        if value.is_object() {
            if host && ["channelPluginBindings", "capabilities"].contains(&key.as_str()) {
                if !value.as_object().unwrap().values().all(|v| {
                    if key == "channelPluginBindings" {
                        v.is_string()
                    } else {
                        v.is_boolean()
                    }
                }) {
                    return Err(invalid());
                }
                merge(saved, key, value);
            } else {
                if !saved[key].is_object() {
                    saved[key] = json!({});
                }
                patch_public(&mut saved[key], value, field, false)?;
            }
        } else if let Some(items) = value.as_array() {
            if has_private_schema(&field["items"]) {
                return Err(invalid());
            }
            let mut checked = Vec::new();
            for item in items {
                validate_type(item, &field["items"])?;
                if item.is_object() {
                    let mut out = json!({});
                    patch_public(&mut out, item, &field["items"], false)?;
                    checked.push(out);
                } else {
                    checked.push(item.clone());
                }
            }
            saved[key] = Value::Array(checked);
        } else {
            saved[key] = value.clone();
        }
    }
    Ok(())
}
fn merge(saved: &mut Value, key: &str, value: &Value) {
    if let Some(fields) = value.as_object() {
        if !saved[key].is_object() {
            saved[key] = json!({});
        }
        for (nested, value) in fields {
            merge(&mut saved[key], nested, value);
        }
    } else {
        saved[key] = value.clone();
    }
}
fn validate_schema_tree(value: &Value, schema: &Value) -> Result<()> {
    validate_type(value, schema)?;
    if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            let field = schema["properties"].get(key).ok_or_else(invalid)?;
            validate_schema_tree(value, field)?;
        }
    } else if let Some(values) = value.as_array() {
        for value in values {
            validate_schema_tree(value, &schema["items"])?;
        }
    }
    Ok(())
}
fn cleared_value(schema: &Value) -> Value {
    match schema["type"].as_str() {
        Some("string") => json!(""),
        Some("object") => json!({}),
        Some("array") => json!([]),
        Some("boolean") => json!(false),
        Some("number" | "integer") => json!(0),
        _ => Value::Null,
    }
}
fn is_cleared(value: &Value) -> bool {
    value.is_null()
        || value.as_str() == Some("")
        || value.as_bool() == Some(false)
        || value.as_f64() == Some(0.0)
        || value.as_array().is_some_and(Vec::is_empty)
        || value.as_object().is_some_and(serde_json::Map::is_empty)
}
fn verify_credential_effect(
    after: &Value,
    input: &Value,
    schema: &Value,
    root: bool,
) -> Result<()> {
    for (key, supplied) in input.as_object().ok_or_else(invalid)? {
        let field = &schema["properties"][key];
        let effective = field["legacyField"]
            .as_str()
            .and_then(|key| after.get(key))
            .filter(|v| !is_cleared(v))
            .unwrap_or(&after[key]);
        let applied = if root && SECRET_FIELDS.iter().any(|(name, _)| *name == key) {
            if supplied.is_null() {
                after[format!("{key}Clear")] == true
            } else {
                &after[key] == supplied
            }
        } else if field["secret"] == true || field["writeOnly"] == true || field["type"] == "array"
        {
            if supplied.is_null() {
                is_cleared(effective)
            } else {
                effective == supplied
            }
        } else {
            verify_credential_effect(&after[key], supplied, field, false)?;
            true
        };
        if !applied {
            return Err(error(
                "CHANNEL_CREDENTIAL_DESTINATION_CHANGED",
                "凭据覆盖未生效，请修改实际继承来源或显式关闭凭据继承",
            ));
        }
    }
    Ok(())
}
fn patch_credentials(saved: &mut Value, input: &Value, schema: &Value, root: bool) -> Result<()> {
    for (key, value) in input.as_object().ok_or_else(invalid)? {
        let field = &schema["properties"][key];
        if root && SECRET_FIELDS.iter().any(|(f, _)| *f == key) {
            if let Some(fields) = schema["properties"].as_object() {
                for (canonical, field) in fields {
                    if field["legacyField"] == *key {
                        saved.as_object_mut().unwrap().shift_remove(canonical);
                    }
                }
            }
            if value.is_null() {
                saved[key] = json!("");
                saved[format!("{key}Clear")] = json!(true);
            } else if value
                .as_str()
                .is_some_and(|v| !v.trim().is_empty() && !v.chars().all(|c| matches!(c, '*' | '•')))
            {
                saved[key] = value.clone();
            } else {
                return Err(invalid());
            }
        } else if field["secret"] == true || field["writeOnly"] == true {
            if let Some(legacy) = field["legacyField"].as_str() {
                saved[legacy] = json!("");
                if root && SECRET_FIELDS.iter().any(|(field, _)| *field == legacy) {
                    saved[format!("{legacy}Clear")] = json!(true);
                }
            }
            if value.is_null() {
                saved[key] = cleared_value(field);
            } else {
                validate_type(value, field)?;
                saved[key] = value.clone();
            }
        } else if field["type"] == "array" && has_private_schema(&field["items"]) {
            if value.is_null() {
                saved[key] = cleared_value(field);
            } else {
                validate_schema_tree(value, field)?;
                saved[key] = value.clone();
            }
        } else if value.is_object() && field["type"] == "object" {
            if !saved[key].is_object() {
                saved[key] = json!({});
            }
            patch_credentials(&mut saved[key], value, field, false)?;
        } else {
            return Err(invalid());
        }
    }
    Ok(())
}
fn has_secret(db: &Connection, key: &str, ctx: &ConfigContext) -> Result<bool> {
    Ok(secrets::dispatch(
        db,
        "secret.exists",
        &json!({"key":key}),
        0,
        ctx,
        &mut Vec::new(),
    )? == true)
}
fn scope_destination_changed(before: &Value, after: &Value) -> bool {
    // The host protocol establishes these fields as local presentation or
    // generation parameters. Unknown package configuration may route requests;
    // retaining an old credential across changes requires an explicit new value.
    const NON_DESTINATION: &[&str] = &[
        "id",
        "name",
        "enabled",
        "model",
        "textModel",
        "imageModel",
        "videoModel",
        "voiceModel",
        "imageModelId",
        "videoModelId",
        "videoResolution",
        "videoAspectRatio",
        "videoDuration",
        "videoCopies",
        "videoTaskConcurrency",
        "videoPollIntervalSeconds",
        "videoPollTimeoutMinutes",
        "imageSize",
        "imageQuality",
        "imageAspectRatio",
        "imageCount",
        "imageMode",
        "textTemperature",
        "textMaxTokens",
        "timeoutMs",
        "maxConcurrent",
        "textTaskConcurrency",
        "imageTaskConcurrency",
        "channelPluginCapabilityConfigs",
    ];
    before
        .as_object()
        .into_iter()
        .flat_map(|m| m.keys())
        .chain(after.as_object().into_iter().flat_map(|m| m.keys()))
        .any(|key| {
            !secret_key(key)
                && !NON_DESTINATION.contains(&key.as_str())
                && before[key] != after[key]
        })
}
fn require_replacement(before: &Value, supplied: &Value, schema: &Value) -> Result<()> {
    fn check(before: &Value, supplied: Option<&Value>, schema: &Value) -> bool {
        if schema["type"] == "array" && supplied == Some(&Value::Null) {
            return true;
        }
        if schema["secret"] == true || schema["writeOnly"] == true {
            return before.is_null() || before.as_str() == Some("") || supplied.is_some();
        }
        if let Some(fields) = schema["properties"].as_object() {
            return fields.iter().all(|(key, field)| {
                let legacy = field["legacyField"].as_str();
                let value = legacy
                    .and_then(|key| before.get(key))
                    .filter(|v| !v.is_null() && v.as_str() != Some(""))
                    .unwrap_or(&before[key]);
                let provided = supplied
                    .and_then(|v| v.get(key))
                    .or_else(|| legacy.and_then(|key| supplied.and_then(|v| v.get(key))));
                check(value, provided, field)
            });
        }
        if let Some(values) = before.as_array() {
            return values
                .iter()
                .enumerate()
                .all(|(i, value)| check(value, supplied.and_then(|v| v.get(i)), &schema["items"]));
        }
        true
    }
    if SECRET_FIELDS.iter().any(|(field, _)| {
        before[format!("{field}Configured")] == true && supplied.get(*field).is_none()
    }) || !check(before, Some(supplied), schema)
    {
        return Err(error(
            "CHANNEL_CREDENTIAL_DESTINATION_CHANGED",
            "更改凭据目的地时须在同一操作显式替换或清除原有密钥",
        ));
    }
    Ok(())
}
fn retained_credentials(before: &Value, after: &Value, schema: &Value) -> Value {
    let mut retained = before.clone();
    for (field, reference) in SECRET_FIELDS {
        let retained_reference = before[reference].as_str().is_some_and(|v| !v.is_empty())
            && before[reference] == after[reference];
        let retained_inline =
            before[field].as_str().is_some_and(|v| !v.is_empty()) && before[field] == after[field];
        retained[format!("{field}Configured")] = json!(
            before[format!("{field}Configured")] == true && (retained_reference || retained_inline)
        );
    }
    fn private_values(before: &Value, after: &Value, schema: &Value) -> Value {
        if schema["secret"] == true || schema["writeOnly"] == true {
            return if before == after {
                before.clone()
            } else {
                Value::Null
            };
        }
        if let Some(fields) = schema["properties"].as_object() {
            let mut value = before.clone();
            if !value.is_object() {
                value = json!({});
            }
            for (key, field) in fields {
                let legacy = field["legacyField"].as_str();
                let old = legacy
                    .and_then(|key| before.get(key))
                    .filter(|v| !v.is_null() && v.as_str() != Some(""))
                    .unwrap_or(&before[key]);
                let next = legacy
                    .and_then(|key| after.get(key))
                    .filter(|v| !v.is_null() && v.as_str() != Some(""))
                    .unwrap_or(&after[key]);
                value[key] = private_values(old, next, field);
                if (field["secret"] == true || field["writeOnly"] == true) && legacy.is_some() {
                    value.as_object_mut().unwrap().shift_remove(legacy.unwrap());
                }
            }
            value
        } else if let Some(values) = before.as_array() {
            Value::Array(
                values
                    .iter()
                    .enumerate()
                    .map(|(i, value)| private_values(value, &after[i], &schema["items"]))
                    .collect(),
            )
        } else {
            before.clone()
        }
    }
    let configured = [
        retained["apiKeyConfigured"].clone(),
        retained["secondaryApiKeyConfigured"].clone(),
    ];
    retained = private_values(&retained, after, schema);
    retained["apiKeyConfigured"] = configured[0].clone();
    retained["secondaryApiKeyConfigured"] = configured[1].clone();
    retained
}
fn provider<'a>(settings: &'a Value, id: &str) -> Result<&'a Value> {
    settings["aiRouting"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == id)
        .ok_or_else(|| error("CHANNEL_NOT_FOUND", "渠道不存在"))
}
fn options_context(
    db: &Connection,
    settings: &Value,
    args: &Value,
    ctx: &ConfigContext,
) -> Result<Value> {
    let cap = args["capability"].as_str().ok_or_else(invalid)?;
    let source = provider(settings, &text(&args["providerId"]))?;
    let id = plugin_id(source, cap);
    let manifest = args["manifests"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["id"] == id)
        .ok_or_else(|| error("PLUGIN_NOT_INSTALLED", "渠道插件尚未安装"))?;
    if manifest["capabilities"][cap] != true {
        return Err(error("PLUGIN_CAPABILITY_UNSUPPORTED", "插件未声明该能力"));
    }
    let action = args["action"].as_str().ok_or_else(invalid)?;
    fn declared(field: &Value, action: &str) -> bool {
        field["optionsSource"]["action"] == action
            || field["properties"]
                .as_object()
                .is_some_and(|fields| fields.values().any(|v| declared(v, action)))
            || field.get("items").is_some_and(|v| declared(v, action))
    }
    if !declared(&manifest["configSchema"], action) {
        return Err(error(
            "PLUGIN_OPTIONS_ACTION_UNKNOWN",
            "插件未声明该配置查询动作",
        ));
    }
    let mut config = channel_capability_config(source, cap);
    for (field, reference) in SECRET_FIELDS {
        if config[field].as_str().unwrap_or("").is_empty() {
            if let Some(key) = config[reference].as_str().filter(|s| !s.is_empty()) {
                config[field] = secrets::get(db, key, ctx)?;
            }
        }
    }
    // A plugin configuration request contains only its declared configuration,
    // not the provider record's other capability credentials or internal refs.
    // The normal plugin host still applies defaults and schema validation.
    let properties = manifest["configSchema"]["properties"]
        .as_object()
        .ok_or_else(invalid)?;
    config
        .as_object_mut()
        .ok_or_else(invalid)?
        .retain(|key, _| {
            properties.contains_key(key)
                || properties
                    .values()
                    .any(|field| field["legacyField"] == *key)
        });
    // This private reply is consumed solely by DesktopApi. It is never a socket
    // method and no descriptor/config snapshot is made public.
    Ok(json!({"pluginId":id,"config":config,"manifest":manifest}))
}
pub(super) fn dispatch(
    db: &Connection,
    op: &str,
    a: &Value,
    now: i64,
    ctx: &ConfigContext,
    events: &mut Vec<Value>,
) -> Result<Value> {
    let mut settings = object(&get(db, "settings", "app")?);
    if op == "config.styles.referencesPatch" {
        let id = a["styleId"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(invalid)?;
        let refs = a["refImages"]
            .as_array()
            .filter(|values| {
                values
                    .iter()
                    .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
            })
            .ok_or_else(invalid)?;
        let styles = settings["globalStyleLibrary"]["styles"]
            .as_array_mut()
            .ok_or_else(|| error("STYLE_PRESET_MISSING", "画风不存在"))?;
        let style = styles
            .iter_mut()
            .find(|s| s["id"] == id)
            .ok_or_else(|| error("STYLE_PRESET_MISSING", "画风不存在"))?;
        style["refImages"] = externalize(&json!(refs), "style-references", ctx)?;
        set(db, "settings", "app", &settings, now)?;
        return Ok(json!({"success":true,"styleId":id,"version":advance(db, now, events)?}));
    }
    if op == "config.channels.optionsContext" {
        return options_context(db, &settings, a, ctx);
    }
    let view = secret_presence_view(&settings, |key| {
        has_secret(db, key, ctx).map_err(|e| aistudio_config_domain::error(&e.code, e.message))
    })?;
    if op == "config.channels.list" {
        return Ok(
            json!({"providers":array(&view["aiRouting"]["providers"]).iter().map(|p| public_provider(p, &a["manifests"])).collect::<Vec<_>>(),"version":version(db)?}),
        );
    }
    if let Some(expected) = a.get("expectedVersion") {
        if expected != &json!(version(db)?) {
            return Err(error(
                "CHANNEL_CONFIG_CONFLICT",
                "渠道设置已更新，请重新读取后修改",
            ));
        }
    }
    let id = a["providerId"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(invalid)?;
    let mut providers = array(&settings["aiRouting"]["providers"]);
    let index = providers.iter().position(|p| p["id"] == id);
    if a["create"] == true && index.is_some() {
        return Err(error("CHANNEL_ALREADY_EXISTS", "渠道已存在"));
    }
    if a["create"] != true && index.is_none() {
        return Err(error("CHANNEL_NOT_FOUND", "渠道不存在"));
    }
    let before = index
        .map(|i| providers[i].clone())
        .unwrap_or_else(|| json!({"id":id}));
    let mut next = before.clone();
    let mut patch = a["patch"].as_object().ok_or_else(invalid)?.clone();
    let capability_patch = patch.shift_remove("channelPluginCapabilityConfigs");
    // Resolve the schema after host binding fields are changed, without trusting
    // a caller-supplied schema. All other fields are checked below.
    for key in [
        "channelPluginId",
        "channelPluginType",
        "channelPluginBindings",
    ] {
        if let Some(v) = patch.get(key) {
            merge(&mut next, key, v);
        }
    }
    let root_schema = schema(&next, "", &a["manifests"]);
    patch_public(&mut next, &Value::Object(patch), &root_schema, true)?;
    if let Some(patches) = capability_patch {
        for (cap, patch) in patches.as_object().ok_or_else(invalid)? {
            let declared = schema(&next, cap, &a["manifests"]);
            if !next["channelPluginCapabilityConfigs"].is_object() {
                next["channelPluginCapabilityConfigs"] = json!({});
            }
            if !next["channelPluginCapabilityConfigs"][cap].is_object() {
                next["channelPluginCapabilityConfigs"][cap] = json!({});
            }
            patch_public(
                &mut next["channelPluginCapabilityConfigs"][cap],
                patch,
                &declared,
                true,
            )?;
        }
    }
    let before_view = view["aiRouting"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == id)
        .cloned()
        .unwrap_or_else(|| before.clone());
    let root_changed = scope_destination_changed(&before, &next);
    if root_changed {
        require_replacement(
            &before_view,
            &a["credentials"],
            &schema(&before, "", &a["manifests"]),
        )?;
    }
    // Compare effective capability configurations, not just existing override
    // records. New overrides and plugin-defined secrets inherit from the root
    // under the host's real capability protocol too.
    let caps: std::collections::BTreeSet<String> = before["channelPluginCapabilityConfigs"]
        .as_object()
        .into_iter()
        .flat_map(|v| v.keys())
        .chain(
            next["channelPluginCapabilityConfigs"]
                .as_object()
                .into_iter()
                .flat_map(|v| v.keys()),
        )
        .cloned()
        .collect();
    let mut supplied = object(&a["credentials"]);
    if !caps.is_empty() {
        supplied["channelPluginCapabilityConfigs"] = json!({});
    }
    for cap in &caps {
        let mut values = object(&a["capabilityCredentials"][cap]);
        values["inheritCredentials"] =
            json!(next["channelPluginCapabilityConfigs"][cap]["inheritCredentials"] == true);
        supplied["channelPluginCapabilityConfigs"][cap] = values;
    }
    for cap in caps {
        let old = channel_capability_config(&before_view, &cap);
        let changed = channel_capability_config(&next, &cap);
        if scope_destination_changed(&old, &changed) {
            let schema = schema(&before, &cap, &a["manifests"]);
            require_replacement(
                &retained_credentials(&old, &changed, &schema),
                &channel_capability_config(&supplied, &cap),
                &schema,
            )?;
        }
    }
    if let Some(credentials) = a.get("credentials") {
        patch_credentials(&mut next, credentials, &root_schema, true)?;
    }
    if let Some(credentials) = a.get("capabilityCredentials") {
        for (cap, values) in credentials.as_object().ok_or_else(invalid)? {
            let declared = schema(&next, cap, &a["manifests"]);
            if !next["channelPluginCapabilityConfigs"].is_object() {
                next["channelPluginCapabilityConfigs"] = json!({});
            }
            if !next["channelPluginCapabilityConfigs"][cap].is_object() {
                next["channelPluginCapabilityConfigs"][cap] = json!({});
            }
            patch_credentials(
                &mut next["channelPluginCapabilityConfigs"][cap],
                values,
                &declared,
                true,
            )?;
        }
    }
    if let Some(credentials) = a.get("credentials") {
        verify_credential_effect(&next, credentials, &root_schema, true)?;
    }
    if let Some(credentials) = a["capabilityCredentials"].as_object() {
        for (cap, values) in credentials {
            verify_credential_effect(
                &channel_capability_config(&next, cap),
                values,
                &schema(&next, cap, &a["manifests"]),
                true,
            )?;
        }
    }
    let envelope = json!({"aiRouting":{"providers":[next]}});
    let writes = collect_secret_writes(&envelope);
    let next = strip_secret_inputs(&envelope)["aiRouting"]["providers"][0].clone();
    if let Some(i) = index {
        providers[i] = next;
    } else {
        providers.push(next);
    }
    if !settings["aiRouting"].is_object() {
        settings["aiRouting"] = json!({});
    }
    settings["aiRouting"]["providers"] = json!(providers);
    secrets::write(db, &json!(writes), now, ctx, events)?;
    set(db, "settings", "app", &settings, now)?;
    Ok(
        json!({"success":true,"providerId":id,"created":a["create"] == true,"version":advance(db, now, events)?}),
    )
}

#[cfg(test)]
mod tests;

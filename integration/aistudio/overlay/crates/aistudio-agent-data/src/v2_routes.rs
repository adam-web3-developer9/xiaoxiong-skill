//! Typed agent operations. All IO goes through the application's guarded Host.
//! Never expose arbitrary IPC, raw settings, secret references or task snapshots.
use crate::{Error, Grant, Host, Result};
use serde_json::{json, Value};

// Pinned to the production generic_owner::PreparationRegistry at 5423be21112.
// The desktop still validates action availability, policy, target and environment.
const ACTIONS: &[&str] = &[
    "videoCaseHumanity",
    "videoCaseEmotion",
    "videoCaseAssets",
    "videoCasePull",
    "videoCaseProduction",
    "videoCaseAssetImage",
    "videoCaseAssetEdit",
    "seriesStructure",
    "scriptGenerate",
    "directorStyleExtraction",
    "promoScript",
    "sceneRefine",
    "scriptInfo",
    "scriptScore",
    "scriptEmotionCurve",
    "viralOpenings",
    "scriptRewrite",
    "requiredAssets",
    "assetExtraction",
    "watermarkRemove",
    "jianyingExport",
    "comicJianyingExport",
    "rhythmRerun",
    "agentChat",
    "modelConnectionTest",
    "libraryAssetValidation",
    "storyboard",
    "voiceMatch",
    "voiceDesign",
    "voiceSample",
    "voiceGeneration",
    "shotVideo",
    "videoPrompt",
    "videoPromptCondense",
    "assetImages",
    "imagePrompt",
    "shotImage",
    "assetImage",
    "assetState",
    "assetRefine",
    "assetValidation",
    "shotRefine",
    "coverGenerate",
    "coverEdit",
    "globalReference",
    "comicStyleReference",
    "previewImageEdit",
    "canvasGenerate",
];
const METHODS: &[(&str, &str)] = &[
    ("capabilities", "read"),
    ("account.status", "read"),
    ("draft.list", "read"),
    ("draft.search", "read"),
    ("draft.get", "read"),
    ("draft.rows", "read"),
    ("draft.apply", "edit"),
    ("draft.create", "edit"),
    ("draft.copy", "edit"),
    ("task.actions", "read"),
    ("task.list", "read"),
    ("task.get", "read"),
    ("task.estimate", "read"),
    ("task.intent", "generate"),
    ("task.cancel", "edit"),
    ("task.retry", "generate"),
    ("task.recheck", "edit"),
    ("batch.get", "read"),
    ("batch.list", "read"),
    ("batch.progress", "read"),
    ("batch.estimate", "read"),
    ("batch.start", "generate"),
    ("batch.pause", "edit"),
    ("batch.stop", "edit"),
    ("batch.resume", "generate"),
    ("batch.retryStage", "generate"),
    ("workflow.list", "read"),
    ("workflow.get", "read"),
    ("workflow.estimate", "read"),
    ("workflow.start", "generate"),
    ("workflow.cancel", "edit"),
    ("workflow.resume", "generate"),
    ("style.list", "read"),
    ("style.setReferences", "edit"),
    ("channels.list", "channels.read"),
    ("channels.get", "channels.read"),
    ("channels.create", "channels.write"),
    ("channels.update", "channels.write"),
    ("channels.models", "channels.read"),
    ("channels.options", "channels.read"),
    ("plugins.list", "read"),
    ("plugins.get", "read"),
    ("plugins.schema", "read"),
    ("plugins.docs", "read"),
    ("plugins.data", "channels.read"),
];
pub fn capabilities() -> Value {
    json!({"methods":METHODS.iter().map(|(method,scope)|json!({"method":method,"scope":scope})).collect::<Vec<_>>(),
        "taskActions":ACTIONS,"credentialRead":false,"credentialWriteScope":"channels.credentials.write",
        "exports":{"via":"task.intent","actions":["jianyingExport","comicJianyingExport"],"scope":"export"},
        "draftSearch":"title/name/description over authorized draft summaries",
        "newDraftsRequireAllDraftsGrant":true})
}
fn bad(message: &str) -> Error {
    Error::new("AGENT_INVALID_REQUEST", message)
}
fn id<'a>(v: &'a Value, field: &str) -> Result<&'a str> {
    v[field]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| bad(&format!("{field} is required")))
}
fn draft_scope(grant: &Grant, draft: &str) -> Result<()> {
    if grant.allows_draft(draft) {
        Ok(())
    } else {
        Err(Error::new(
            "AGENT_DRAFT_DENIED",
            "Draft is outside this application's grant",
        ))
    }
}
fn all_drafts(grant: &Grant) -> Result<()> {
    if grant.draft_ids.iter().any(|s| s == "*") {
        Ok(())
    } else {
        Err(Error::new(
            "AGENT_DRAFT_DENIED",
            "This operation requires an all-drafts grant",
        ))
    }
}
fn owned(grant: &Grant, object: &Value, field: &str) -> Result<()> {
    match object[field].as_str().filter(|s| !s.is_empty()) {
        Some(draft) => draft_scope(grant, draft),
        None => all_drafts(grant),
    }
}
fn pick(value: &Value, fields: &[&str]) -> Value {
    let mut out = serde_json::Map::new();
    for field in fields {
        if let Some(v) = value.get(*field) {
            out.insert((*field).into(), v.clone());
        }
    }
    Value::Object(out)
}
fn private_field(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace(['_', '-'], "");
    [
        "apikey",
        "privatekey",
        "secret",
        "token",
        "cookie",
        "password",
        "authorization",
        "fingerprint",
        "credential",
        "valuebase64",
        "vaultpath",
        "databasepath",
        "dbpath",
        "secretsfile",
    ]
    .iter()
    .any(|s| key.contains(s))
}
/// Secondary recursive boundary for ordinary business data, not a replacement
/// for the explicit channel/account/task allowlists below.
fn public_data(v: &Value) -> Value {
    match v {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(k, _)| !private_field(k))
                .map(|(k, v)| (k.clone(), public_data(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(public_data).collect()),
        _ => v.clone(),
    }
}
fn safe_failure(error: Error) -> Error {
    // Provider errors can echo request bodies and credentials even in message.
    // Only stable code identifiers cross this boundary; diagnostics stay in app.
    let code = if !error.code.is_empty()
        && error
            .code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        error.code
    } else {
        "AGENT_COMMAND_FAILED".into()
    };
    Error::new(&code, failure_message(&code))
}
async fn call(host: &dyn Host, client: &str, channel: &str, args: Vec<Value>) -> Result<Value> {
    let result = host
        .invoke(channel, client, args)
        .await
        .map_err(safe_failure)?;
    if result["__aistudioIpcError"] == 1 || (channel == "data:apply" && result["ok"] == false) {
        return Err(safe_failure(Error::new(
            result["error"]["code"]
                .as_str()
                .unwrap_or("AGENT_COMMAND_FAILED"),
            "Application operation failed",
        )));
    }
    Ok(if channel == "data:apply" && result["ok"] == true {
        result["result"].clone()
    } else {
        result
    })
}
fn failure_message(code: &str) -> &'static str {
    match code {
        "CONTENT_REJECTED" | "CONTENT_MODERATION_FAILED" => {
            "内容未通过供应商审核，请在应用内查看审核原因并修改提示词或参考素材。"
        }
        "RATE_LIMITED" | "RATE_LIMIT_EXCEEDED" => "供应商限流，请稍后重试或调整渠道并发。",
        "INSUFFICIENT_BALANCE" | "INSUFFICIENT_CREDITS" => {
            "渠道余额或额度不足，请在应用内核对账户。"
        }
        "TASK_CANCELLED" => "任务已取消。",
        "PROVIDER_TIMEOUT" | "REQUEST_TIMEOUT" => {
            "供应商响应超时。先续查已有任务；重新生成可能再次扣费。"
        }
        "AGENT_DRAFT_DENIED" | "ACCOUNT_ACCESS_DENIED" => "当前授权不允许访问这个目标。",
        "VERSION_CONFLICT" | "DATA_VERSION_CONFLICT" => "数据版本发生变化，请重新读取目标后提交。",
        _ => "应用操作失败，请结合错误代码在应用内查看详情。",
    }
}
fn task_view(v: &Value) -> Value {
    if v.is_null() {
        return Value::Null;
    }
    let mut out = public_data(&pick(
        v,
        &[
            "id",
            "type",
            "status",
            "targetDraftId",
            "targetType",
            "targetId",
            "targetField",
            "targetEpisodeId",
            "targetShotId",
            "targetAssetId",
            "targetNodeId",
            "progress",
            "createdAt",
            "updatedAt",
            "startedAt",
            "finishedAt",
            "completedAt",
            "durationMs",
            "attempt",
            "attemptId",
            "reusedExisting",
            "phase",
            "providerStatus",
            "modelName",
            "resultGenerationId",
            "resultUrl",
            "canCancel",
            "canRetry",
            "canRecheck",
            "canRecheckReceipt",
        ],
    ));
    if v["outputSnapshot"].is_object() {
        out["outputSnapshot"] = pick(&v["outputSnapshot"], &["localUrl", "originalUrl"]);
    }
    if !v["error"].is_null() {
        let code = v["error"]["code"]
            .as_str()
            .filter(|c| {
                c.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            })
            .unwrap_or("TASK_FAILED");
        out["error"] = pick(&v["error"], &["class", "retryable"]);
        out["error"]["code"] = json!(code);
        out["error"]["message"] = json!(failure_message(code));
    }
    out
}
fn task_result(v: &Value) -> Value {
    if v.get("tasks").is_some() {
        let mut result = pick(
            v,
            &[
                "requestId",
                "taskIds",
                "createdCount",
                "reusedCount",
                "reusedExisting",
            ],
        );
        result["tasks"] = json!(v["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(task_view)
            .collect::<Vec<_>>());
        result
    } else if v.get("id").is_some() {
        task_view(v)
    } else {
        public_data(&pick(
            v,
            &[
                "taskId",
                "taskIds",
                "requestId",
                "success",
                "cancelled",
                "changed",
                "reusedExisting",
            ],
        ))
    }
}
async fn get_task(host: &dyn Host, client: &str, task_id: &str) -> Result<Value> {
    let tasks = call(host, client, "local-data:tasks-list", vec![]).await?;
    let list = tasks
        .as_array()
        .ok_or_else(|| bad("Application task list is invalid"))?;
    if let Some(task) = list.iter().find(|v| v["id"] == task_id) {
        return Ok(task.clone());
    }
    let history = call(host, client, "task-history:detail", vec![json!(task_id)]).await?;
    if history.is_null() {
        return Ok(Value::Null);
    }
    if !history["task"].is_object() {
        return Err(Error::new(
            "AGENT_TASK_SNAPSHOT_UNAVAILABLE",
            "Task ownership snapshot is unavailable",
        ));
    }
    Ok(history["task"].clone())
}
fn export_action(action: &str) -> bool {
    matches!(action, "jianyingExport" | "comicJianyingExport")
}
fn global_action(action: &str) -> bool {
    action.starts_with("videoCase")
        || matches!(
            action,
            "directorStyleExtraction"
                | "modelConnectionTest"
                | "libraryAssetValidation"
                | "globalReference"
                | "comicStyleReference"
                | "previewImageEdit"
                | "canvasGenerate"
                | "agentChat"
        )
}
async fn intent_scope(
    host: &dyn Host,
    client: &str,
    params: &Value,
    grant: &Grant,
    write: bool,
) -> Result<()> {
    let action = id(params, "action")?;
    if !ACTIONS.contains(&action) {
        return Err(bad("Unknown task action"));
    }
    if global_action(action) {
        all_drafts(grant)?;
    } else if let Some(draft) = params["target"]["draftId"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        draft_scope(grant, draft)?;
        if call(host, client, "local-data:drafts-get", vec![json!(draft)])
            .await?
            .is_null()
        {
            return Err(Error::new("AGENT_ENTITY_NOT_FOUND", "Draft was not found"));
        }
    }
    if params["target"]["draftId"]
        .as_str()
        .is_none_or(|s| s.is_empty())
    {
        all_drafts(grant)?;
    }
    if write && export_action(action) {
        grant.requires_scope("export")?;
    }
    if let Some(source) = params["options"]["sourceTaskId"].as_str() {
        let task = get_task(host, client, source).await?;
        if task.is_null() {
            return Err(Error::new(
                "AGENT_ENTITY_NOT_FOUND",
                "Source task was not found",
            ));
        }
        owned(grant, &task, "targetDraftId")?;
    }
    Ok(())
}
// Provider fields are intentionally enumerated, never "remove a few secrets".
const CHANNEL_FIELDS: &[&str] = &[
    "id",
    "name",
    "enabled",
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
    "channelPluginVersion",
    "channelPluginType",
    "channelPluginBindings",
    "inheritCredentials",
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
];
fn channel_patch(patch: &Value) -> Result<()> {
    let fields = patch
        .as_object()
        .ok_or_else(|| bad("patch must be an object"))?;
    for (key, value) in fields {
        if key == "channelPluginBindings" {
            let bindings = value
                .as_object()
                .ok_or_else(|| bad("channelPluginBindings must be an object"))?;
            if bindings.iter().any(|(k, v)| {
                ![
                    "text",
                    "image",
                    "video",
                    "voice",
                    "audio",
                    "storage",
                    "watermark",
                ]
                .contains(&k.as_str())
                    || !v.is_string()
            }) {
                return Err(bad("Invalid capability binding"));
            }
        } else if key == "channelPluginCapabilityConfigs" {
            let configs = value
                .as_object()
                .ok_or_else(|| bad("Capability configuration must be an object"))?;
            for (cap, config) in configs {
                if ![
                    "text",
                    "image",
                    "video",
                    "voice",
                    "audio",
                    "storage",
                    "watermark",
                ]
                .contains(&cap.as_str())
                {
                    return Err(bad("Unsupported capability"));
                }
                channel_patch(config)?;
            }
        } else if key == "id"
            || (private_field(key) && key != "inheritCredentials")
            || (CHANNEL_FIELDS.contains(&key.as_str()) && value.is_object())
        {
            return Err(bad(
                "Channel patch contains a non-public or unsupported field",
            ));
        }
    }
    Ok(())
}
fn credentials(value: &Value) -> Result<()> {
    if !value.is_object() {
        return Err(bad("credentials must be an object"));
    }
    // The typed owner validates fields against its installed manifest and writes
    // them in the same transaction. The agent never accepts a client schema.
    Ok(())
}
fn schema_view(v: &Value) -> Value {
    schema_view_inner(v, false)
}
fn schema_view_inner(v: &Value, inherited_secret: bool) -> Value {
    let mut out = pick(
        v,
        &[
            "type",
            "title",
            "description",
            "required",
            "additionalProperties",
        ],
    );
    let secret = inherited_secret || v["secret"] == true || v["writeOnly"] == true;
    out["writeOnly"] = json!(secret);
    if !secret {
        for key in [
            "default",
            "enum",
            "optionsSource",
            "browserLogin",
            "legacyField",
        ] {
            if let Some(value) = v.get(key) {
                out[key] = match key {
                    "default" => schema_config(v, value),
                    "enum" => json!(value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|item| schema_config(v, item))
                        .collect::<Vec<_>>()),
                    _ => public_data(value),
                };
            }
        }
    }
    if let Some(fields) = v["properties"].as_object() {
        out["properties"] = Value::Object(
            fields
                .iter()
                .map(|(key, field)| {
                    let mut field = field.clone();
                    if private_field(key) {
                        field["secret"] = json!(true);
                    }
                    (key.clone(), schema_view_inner(&field, secret))
                })
                .collect(),
        );
    }
    if v["items"].is_object() {
        out["items"] = schema_view_inner(&v["items"], secret);
    }
    out
}
fn schema_config(schema: &Value, value: &Value) -> Value {
    if schema["secret"] == true || schema["writeOnly"] == true {
        return Value::Null;
    }
    if let Some(fields) = schema["properties"].as_object() {
        return Value::Object(
            fields
                .iter()
                .filter(|(key, field)| {
                    !private_field(key) && field["secret"] != true && field["writeOnly"] != true
                })
                .filter_map(|(key, field)| {
                    value
                        .get(key)
                        .map(|v| (key.clone(), schema_config(field, v)))
                })
                .collect(),
        );
    }
    if let Some(items) = value.as_array() {
        return json!(items
            .iter()
            .map(|v| schema_config(&schema["items"], v))
            .collect::<Vec<_>>());
    }
    public_data(value)
}
fn plugin_view(v: &Value) -> Value {
    let mut out = public_data(&pick(
        v,
        &[
            "id",
            "name",
            "version",
            "description",
            "icon",
            "protocolVersion",
            "runtime",
            "ipc",
            "target",
            "capabilities",
            "legacyReplacement",
            "providerInfo",
            "capabilityDetails",
            "installed",
            "installedVersion",
            "enabled",
            "source",
            "packageDigest",
            "versionPinned",
        ],
    ));
    out["configSchema"] = schema_view(&v["configSchema"]);
    if let Some(versions) = v["versions"].as_array() {
        out["versions"] = json!(versions.iter().map(plugin_view).collect::<Vec<_>>());
    }
    if let Some(presets) = v["channelPresets"].as_array() {
        out["channelPresets"]=json!(presets.iter().map(|p|json!({"id":p["id"],"name":p["name"],"config":schema_config(&v["configSchema"],&p["config"])})).collect::<Vec<_>>());
    }
    out
}
async fn settings(host: &dyn Host, client: &str) -> Result<Value> {
    call(
        host,
        client,
        "local-data:kv-get",
        vec![json!("settings"), json!("app")],
    )
    .await
}
fn orchestration_view(value: &Value) -> Value {
    if value.is_null() {
        return Value::Null;
    }
    if let Some(items) = value.as_array() {
        return json!(items.iter().map(orchestration_view).collect::<Vec<_>>());
    }
    let mut out = pick(
        value,
        &[
            "id",
            "runId",
            "workflowId",
            "draftId",
            "kind",
            "status",
            "executionStatus",
            "version",
            "createdAt",
            "updatedAt",
            "startedAt",
            "completedAt",
            "nextActionAt",
            "episodeIds",
            "stageKeys",
            "episodeId",
            "stage",
            "stepKey",
            "taskId",
            "taskIds",
            "previousTaskIds",
            "attempt",
            "manualRetries",
            "retryAt",
            "nextRunAt",
            "done",
            "total",
            "failed",
            "running",
            "blocked",
            "pending",
            "percent",
            "success",
            "changed",
        ],
    );
    if value["target"].is_object() {
        out["target"] = pick(&value["target"], &["draftId", "entity", "episodeId", "ids"]);
    }
    for key in ["summary", "environment"] {
        if value[key].is_object() {
            out[key] = orchestration_view(&value[key]);
        }
    }
    for key in ["steps", "episodes", "stages"] {
        if let Some(fields) = value[key].as_object() {
            out[key] = Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), orchestration_view(v)))
                    .collect(),
            );
        } else if value[key].is_array() {
            out[key] = orchestration_view(&value[key]);
        }
    }
    for key in ["error", "blockedReason"] {
        if !value[key].is_null() && value[key] != "" {
            let code = value[key]["code"]
                .as_str()
                .filter(|c| {
                    c.bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                })
                .unwrap_or("WORKFLOW_BLOCKED");
            out[key] = pick(&value[key], &["stepKey", "taskId", "retryable", "class"]);
            out[key]["code"] = json!(code);
            out[key]["message"] = json!(failure_message(code));
        }
    }
    out
}
fn referenced_tasks(value: &Value, ids: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, v) in fields {
                if matches!(key.as_str(), "taskId" | "sourceTaskId" | "seedTaskId") {
                    if let Some(id) = v.as_str() {
                        ids.push(id.into());
                    }
                } else if matches!(key.as_str(), "taskIds" | "seedTaskIds" | "previousTaskIds") {
                    if let Some(items) = v.as_array() {
                        ids.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
                    }
                } else {
                    referenced_tasks(v, ids);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                referenced_tasks(item, ids);
            }
        }
        _ => {}
    }
}
async fn check_referenced_tasks(
    host: &dyn Host,
    client: &str,
    params: &Value,
    grant: &Grant,
) -> Result<()> {
    let mut ids = vec![];
    referenced_tasks(params, &mut ids);
    ids.sort();
    ids.dedup();
    for id in ids {
        let task = get_task(host, client, &id).await?;
        if task.is_null() {
            return Err(Error::new(
                "AGENT_ENTITY_NOT_FOUND",
                "Referenced task was not found",
            ));
        }
        owned(grant, &task, "targetDraftId")?;
    }
    Ok(())
}
async fn batch_run(host: &dyn Host, client: &str, params: &Value, grant: &Grant) -> Result<Value> {
    let draft = id(params, "draftId")?;
    draft_scope(grant, draft)?;
    let run = call(host, client, "batch:get", vec![json!(draft)]).await?;
    if run.is_null() {
        return Err(Error::new(
            "AGENT_ENTITY_NOT_FOUND",
            "Batch run was not found",
        ));
    }
    // Never authorize a run by the caller's supplied draftId alone.
    owned(grant, &run, "draftId")?;
    if run["draftId"] != draft || run["runId"] != params["runId"] {
        return Err(Error::new(
            "AGENT_DRAFT_DENIED",
            "Batch run does not belong to the requested draft",
        ));
    }
    Ok(run)
}
pub(crate) async fn invoke(
    host: &dyn Host,
    client: &str,
    method: &str,
    params: &Value,
    grant: &Grant,
) -> Result<Value> {
    let scope = METHODS
        .iter()
        .find(|(m, _)| *m == method)
        .map(|(_, s)| *s)
        .ok_or_else(|| Error::new("AGENT_METHOD_NOT_ALLOWED", "Agent method is not allowed"))?;
    grant.requires_scope(scope)?;
    if !params.is_object() {
        return Err(bad("params must be an object"));
    }
    match method {
        "capabilities" => Ok(capabilities()),
        "account.status" => {
            let v = call(host, client, "account:status", vec![]).await?;
            Ok(
                json!({"sessionPresent":v["sessionPresent"]==true,"localAllowed":v["localAllowed"]==true,"cloudAllowed":v["cloudAllowed"]==true,"emailVerified":v["profile"]["emailVerified"]==true,"everActivated":v["profile"]["everActivated"]==true}),
            )
        }
        "draft.list" | "draft.search" => {
            let list = call(host, client, "local-data:drafts-list-summaries", vec![]).await?;
            let rows = list
                .as_array()
                .ok_or_else(|| bad("Application draft summaries are invalid"))?;
            let query = params["query"].as_str().unwrap_or("").to_lowercase();
            Ok(json!(rows
                .iter()
                .filter(|d| d["id"].as_str().is_some_and(|id| grant.allows_draft(id)))
                .filter(|d| method != "draft.search"
                    || ["title", "name", "description", "textPreview"]
                        .iter()
                        .any(|key| d[*key]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)))
                .map(public_data)
                .collect::<Vec<_>>()))
        }
        "draft.get" | "draft.rows" | "draft.apply" => {
            draft_scope(grant, id(params, "draftId")?)?;
            // Existing command origin, CAS and no-skip constraints remain intact.
            crate::routes::invoke(host, client, method, params)
                .await
                .map(|v| public_data(&v))
                .map_err(safe_failure)
        }
        "draft.create" => {
            all_drafts(grant)?;
            if !params["draft"].is_object() {
                return Err(bad("draft must be an object"));
            }
            let v = call(
                host,
                client,
                "local-data:drafts-create",
                vec![params["draft"].clone()],
            )
            .await?;
            Ok(public_data(&v))
        }
        "draft.copy" => {
            all_drafts(grant)?;
            draft_scope(grant, id(params, "draftId")?)?;
            let v = call(
                host,
                client,
                "local-data:drafts-duplicate",
                vec![
                    params["draftId"].clone(),
                    params.get("options").cloned().unwrap_or(json!({})),
                ],
            )
            .await?;
            Ok(public_data(&v))
        }
        "task.actions" => Ok(
            json!({"actions":ACTIONS.iter().map(|action|json!({"action":action,"scope":if export_action(action){"generate+export"}else{"generate"},"allDraftsGrant":global_action(action)})).collect::<Vec<_>>()}),
        ),
        "task.list" => {
            let tasks = call(host, client, "local-data:tasks-list", vec![]).await?;
            let tasks = tasks
                .as_array()
                .ok_or_else(|| bad("Application task list is invalid"))?;
            Ok(json!(tasks
                .iter()
                .filter(|t| owned(grant, t, "targetDraftId").is_ok())
                .filter(|t| [
                    ("draftId", "targetDraftId"),
                    ("status", "status"),
                    ("type", "type")
                ]
                .iter()
                .all(|(a, b)| params.get(*a).is_none() || params[*a] == t[*b]))
                .map(task_view)
                .collect::<Vec<_>>()))
        }
        "task.get" | "task.cancel" | "task.retry" | "task.recheck" => {
            let task_id = id(params, "taskId")?;
            let task = get_task(host, client, task_id).await?;
            if task.is_null() {
                return Ok(Value::Null);
            }
            owned(grant, &task, "targetDraftId")?;
            if method == "task.get" {
                return Ok(task_view(&task));
            }
            if method == "task.retry" && matches!(task["type"].as_str(), Some("export")) {
                grant.requires_scope("export")?;
            }
            let channel = match method {
                "task.cancel" => "ai-task:cancel",
                "task.retry" => "ai-task:retry",
                _ => "ai-task:recheck",
            };
            Ok(task_result(
                &call(host, client, channel, vec![json!(task_id)]).await?,
            ))
        }
        "task.estimate" | "task.intent" => {
            intent_scope(host, client, params, grant, method == "task.intent").await?;
            if params["options"]["manual"] == true {
                return Err(bad(
                    "Agent task intents must retain automatic target deduplication",
                ));
            }
            id(params, "requestId")?;
            let v = call(
                host,
                client,
                if method == "task.intent" {
                    "ai-task:intent"
                } else {
                    "ai-task:estimate"
                },
                vec![params.clone()],
            )
            .await?;
            Ok(if method == "task.intent" {
                task_result(&v)
            } else {
                public_data(&v)
            })
        }
        "style.list" => {
            let v = settings(host, client).await?;
            Ok(json!(v["globalStyleLibrary"]["styles"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| public_data(&pick(s, &["id", "name", "description", "refImages"])))
                .collect::<Vec<_>>()))
        }
        "style.setReferences" => {
            id(params, "styleId")?;
            if !params["refImages"].as_array().is_some_and(|a| {
                a.iter()
                    .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
            }) {
                return Err(bad("refImages must contain nonempty strings"));
            }
            Ok(public_data(
                &call(
                    host,
                    client,
                    "local-data:style-references-patch",
                    vec![pick(params, &["styleId", "refImages"])],
                )
                .await?,
            ))
        }
        "channels.list" | "channels.get" => {
            let v = call(host, client, "local-data:channel-config-list", vec![]).await?;
            let providers = v["providers"]
                .as_array()
                .ok_or_else(|| bad("Channel configuration is unavailable"))?;
            let versioned = |provider: &Value| {
                let mut provider = provider.clone();
                if provider.is_object() && v["version"].is_number() {
                    provider["configurationVersion"] = v["version"].clone();
                }
                provider
            };
            if method == "channels.list" {
                Ok(json!(providers.iter().map(versioned).collect::<Vec<_>>()))
            } else {
                let wanted = id(params, "providerId")?;
                Ok(providers
                    .iter()
                    .find(|v| v["id"] == wanted)
                    .map(versioned)
                    .unwrap_or(Value::Null))
            }
        }
        "channels.create" | "channels.update" => {
            let provider = id(params, "providerId")?;
            channel_patch(&params["patch"])?;
            if let Some(creds) = params.get("credentials") {
                grant.requires_scope("channels.credentials.write")?;
                credentials(creds)?;
            }
            if let Some(configs) = params.get("capabilityCredentials") {
                grant.requires_scope("channels.credentials.write")?;
                let configs = configs
                    .as_object()
                    .ok_or_else(|| bad("capabilityCredentials must be an object"))?;
                for (cap, creds) in configs {
                    if ![
                        "text",
                        "image",
                        "video",
                        "voice",
                        "audio",
                        "storage",
                        "watermark",
                    ]
                    .contains(&cap.as_str())
                    {
                        return Err(bad("Unsupported credential capability"));
                    }
                    credentials(creds)?;
                }
            }
            if params
                .get("expectedVersion")
                .is_some_and(|v| !v.is_number())
            {
                return Err(bad("expectedVersion must be a number"));
            }
            let mut request = pick(
                params,
                &[
                    "patch",
                    "credentials",
                    "capabilityCredentials",
                    "expectedVersion",
                ],
            );
            request["providerId"] = json!(provider);
            request["create"] = json!(method == "channels.create");
            // Owner does atomic deep merge, preserving unknown existing fields and
            // credential references; secret input never goes through ordinary kv-set.
            let result = call(
                host,
                client,
                "local-data:channel-config-patch",
                vec![request],
            )
            .await?;
            // Never return a raw write handler result: it may echo input credentials.
            let mut response = json!({"success":true,"providerId":provider});
            if result["version"].is_number() {
                response["version"] = result["version"].clone();
            }
            Ok(response)
        }
        "channels.models" | "channels.options" | "plugins.data" => {
            id(params, "providerId")?;
            id(params, "capability")?;
            if params
                .as_object()
                .unwrap()
                .keys()
                .any(|k| !["providerId", "capability", "action"].contains(&k.as_str()))
            {
                return Err(bad(
                    "Model discovery accepts only providerId, capability and action",
                ));
            }
            let action = if method == "channels.models" {
                "models"
            } else {
                id(params, "action")?
            };
            let v=call(host,client,"local-data:channel-config-options",vec![json!({"providerId":params["providerId"],"capability":params["capability"],"action":action})]).await?;
            Ok(public_data(&v))
        }
        "plugins.list" | "plugins.get" | "plugins.schema" | "plugins.docs" => {
            let catalog = call(host, client, "channel-plugins:catalog", vec![]).await?;
            let installed = call(host, client, "channel-plugins:list", vec![]).await?;
            let rows = catalog
                .as_array()
                .ok_or_else(|| bad("Application plugin catalog is invalid"))?;
            let installs = installed
                .as_array()
                .ok_or_else(|| bad("Application plugin list is invalid"))?;
            let mut rows = rows.clone();
            for plugin in installs {
                if let Some(row) = rows.iter_mut().find(|p| p["id"] == plugin["id"]) {
                    let mut merged = plugin.clone();
                    for key in ["source", "versions", "installedVersion"] {
                        if let Some(v) = row.get(key) {
                            merged[key] = v.clone();
                        }
                    }
                    *row = merged;
                } else {
                    rows.push(plugin.clone());
                }
            }
            if method == "plugins.list" {
                return Ok(json!(rows.iter().map(plugin_view).collect::<Vec<_>>()));
            }
            let wanted = id(params, "pluginId")?;
            let Some(plugin) = rows.iter().find(|p| p["id"] == wanted) else {
                return Ok(Value::Null);
            };
            let view = plugin_view(plugin);
            Ok(match method {
                "plugins.schema" => view["configSchema"].clone(),
                "plugins.docs" => pick(
                    &view,
                    &[
                        "id",
                        "name",
                        "description",
                        "providerInfo",
                        "capabilityDetails",
                    ],
                ),
                _ => view,
            })
        }
        "batch.list" => {
            let v = call(host, client, "batch:list-active", vec![]).await?;
            let rows = v
                .as_array()
                .ok_or_else(|| bad("Application batch list is invalid"))?;
            Ok(json!(rows
                .iter()
                .filter(|v| owned(grant, v, "draftId").is_ok())
                .map(orchestration_view)
                .collect::<Vec<_>>()))
        }
        "batch.get" | "batch.estimate" | "batch.start" => {
            let draft = id(params, "draftId")?;
            draft_scope(grant, draft)?;
            let (channel, args) = match method {
                "batch.get" => ("batch:get", vec![json!(draft)]),
                "batch.estimate" => ("batch:estimate", vec![json!(draft), params["run"].clone()]),
                _ => ("batch:start", vec![json!(draft), params["run"].clone()]),
            };
            let v = call(host, client, channel, args).await?;
            Ok(if method == "batch.estimate" {
                public_data(&v)
            } else {
                orchestration_view(&v)
            })
        }
        "batch.progress" | "batch.pause" | "batch.stop" | "batch.resume" | "batch.retryStage" => {
            id(params, "runId")?;
            batch_run(host, client, params, grant).await?;
            let mut args = vec![params["runId"].clone()];
            let channel = match method {
                "batch.progress" => "batch:progress",
                "batch.pause" => "batch:pause",
                "batch.stop" => "batch:stop",
                "batch.resume" => "batch:resume",
                _ => {
                    id(params, "episodeId")?;
                    id(params, "stage")?;
                    args.extend([params["episodeId"].clone(), params["stage"].clone()]);
                    "batch:retry-stage"
                }
            };
            Ok(orchestration_view(
                &call(host, client, channel, args).await?,
            ))
        }
        "workflow.list" => {
            let draft = id(params, "draftId")?;
            draft_scope(grant, draft)?;
            let v = call(host, client, "workflow:list", vec![json!(draft), json!({})]).await?;
            let rows = v
                .as_array()
                .ok_or_else(|| bad("Application workflow list is invalid"))?;
            Ok(json!(rows
                .iter()
                .filter(|v| owned(grant, v, "draftId").is_ok()
                    && (!matches!(
                        v["kind"].as_str(),
                        Some("derivedDraftFromCase" | "videoCaseAnalysis")
                    ) || all_drafts(grant).is_ok()))
                .map(orchestration_view)
                .collect::<Vec<_>>()))
        }
        "workflow.start" | "workflow.estimate" => {
            if matches!(
                params["kind"].as_str(),
                Some("derivedDraftFromCase" | "videoCaseAnalysis")
            ) {
                all_drafts(grant)?;
            } else {
                draft_scope(grant, id(&params["target"], "draftId")?)?;
            }
            check_referenced_tasks(host, client, params, grant).await?;
            let v = call(
                host,
                client,
                if method == "workflow.start" {
                    "workflow:start"
                } else {
                    "workflow:estimate"
                },
                vec![params.clone()],
            )
            .await?;
            Ok(if method == "workflow.estimate" {
                public_data(&v)
            } else {
                orchestration_view(&v)
            })
        }
        "workflow.get" | "workflow.cancel" | "workflow.resume" => {
            let workflow = id(params, "workflowId")?;
            let v = call(host, client, "workflow:get", vec![json!(workflow)]).await?;
            if v.is_null() {
                return Ok(v);
            }
            owned(grant, &v, "draftId")?;
            if matches!(
                v["kind"].as_str(),
                Some("derivedDraftFromCase" | "videoCaseAnalysis")
            ) {
                all_drafts(grant)?;
            }
            if method == "workflow.get" {
                return Ok(orchestration_view(&v));
            }
            let mut args = vec![json!(workflow)];
            if method == "workflow.resume" {
                check_referenced_tasks(host, client, &v, grant).await?;
                args.push(json!(id(params, "stepKey")?));
            }
            Ok(orchestration_view(
                &call(
                    host,
                    client,
                    if method == "workflow.cancel" {
                        "workflow:cancel"
                    } else {
                        "workflow:resume"
                    },
                    args,
                )
                .await?,
            ))
        }
        _ => Err(Error::new(
            "AGENT_METHOD_NOT_ALLOWED",
            "Agent method is not allowed",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{v2_auth::test_grant, HostFuture};
    use std::{collections::VecDeque, sync::Mutex};
    struct Fake {
        responses: Mutex<VecDeque<(String, Value)>>,
        calls: Mutex<Vec<(String, Vec<Value>)>>,
    }
    impl Fake {
        fn new(responses: Vec<(&str, Value)>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().map(|(k, v)| (k.into(), v)).collect()),
                calls: Mutex::new(vec![]),
            }
        }
    }
    impl Host for Fake {
        fn build_snapshot(&self, _: &str) -> Value {
            json!({})
        }
        fn verify_build(&self, _: &str, _: &str) -> Result<Value> {
            Ok(json!({}))
        }
        fn forget_build(&self, _: &str) {}
        fn invoke<'a>(
            &'a self,
            channel: &'a str,
            _: &'a str,
            args: Vec<Value>,
        ) -> HostFuture<'a, Value> {
            Box::pin(async move {
                self.calls.lock().unwrap().push((channel.into(), args));
                let (expected, result) = self
                    .responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected host invocation");
                assert_eq!(channel, expected);
                Ok(result)
            })
        }
    }
    #[test]
    fn catalog_has_48_real_actions_and_no_secret_read() {
        assert_eq!(ACTIONS.len(), 48);
        assert!(!METHODS
            .iter()
            .any(|(m, _)| m.contains("reveal") || m.contains("secret.get")));
        assert_eq!(capabilities()["credentialRead"], false);
    }
    #[tokio::test]
    async fn channels_read_only_typed_owner_projection() {
        let host = Fake::new(vec![(
            "local-data:channel-config-list",
            json!({"providers":[{"id":"p","customPublicField":"visible","credentialsConfigured":{"primary":true}}],"version":1}),
        )]);
        let result = invoke(
            &host,
            "c",
            "channels.get",
            &json!({"providerId":"p"}),
            &test_grant(&["channels.read"], &[]),
        )
        .await
        .unwrap();
        assert_eq!(result["customPublicField"], "visible");
        assert_eq!(
            host.calls.lock().unwrap()[0].0,
            "local-data:channel-config-list"
        );
    }
    #[test]
    fn ordinary_patch_cannot_replace_secrets_or_inject_unknown_objects() {
        for patch in [
            json!({"apiKey":"x"}),
            json!({"secretRef":"x"}),
            json!({"channelPluginCapabilityConfigs":{"image":{"apiKey":"x"}}}),
            json!({"baseUrl":{"apiKey":"x"}}),
        ] {
            assert!(channel_patch(&patch).is_err());
        }
        assert!(channel_patch(
            &json!({"name":"A","channelPluginCapabilityConfigs":{"image":{"imageModel":"x"}}})
        )
        .is_ok());
        assert!(channel_patch(&json!({"channelPluginBindings":{"image":"official-a"}})).is_ok());
    }
    #[tokio::test]
    async fn task_control_resolves_actual_owner_before_mutating() {
        let host = Fake::new(vec![(
            "local-data:tasks-list",
            json!([{"id":"t","targetDraftId":"other","inputSnapshot":{"apiKey":"LEAK"}}]),
        )]);
        let grant = test_grant(&["edit"], &["mine"]);
        let result = invoke(
            &host,
            "client",
            "task.cancel",
            &json!({"taskId":"t","draftId":"mine"}),
            &grant,
        )
        .await;
        assert_eq!(result.unwrap_err().code, "AGENT_DRAFT_DENIED");
        assert_eq!(host.calls.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn retry_requires_generate_before_any_lookup() {
        let host = Fake::new(vec![]);
        let grant = test_grant(&["edit"], &["*"]);
        assert_eq!(
            invoke(
                &host,
                "client",
                "task.retry",
                &json!({"taskId":"t"}),
                &grant
            )
            .await
            .unwrap_err()
            .code,
            "AGENT_SCOPE_DENIED"
        );
    }
    #[tokio::test]
    async fn task_list_filters_scope_and_never_returns_snapshot() {
        let host = Fake::new(vec![(
            "local-data:tasks-list",
            json!([{"id":"t","targetDraftId":"mine","inputSnapshot":{"apiKey":"LEAK"},"message":"LEAK"},{"id":"other","targetDraftId":"other"}]),
        )]);
        let result = invoke(
            &host,
            "client",
            "task.list",
            &json!({}),
            &test_grant(&["read"], &["mine"]),
        )
        .await
        .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 1);
        assert!(!result.to_string().contains("LEAK"));
    }
    #[tokio::test]
    async fn credential_write_is_separately_granted_and_never_echoed() {
        let params = json!({"providerId":"a","patch":{"name":"A"},"credentials":{"apiKey":"LEAK"}});
        let host = Fake::new(vec![]);
        assert_eq!(
            invoke(
                &host,
                "c",
                "channels.create",
                &params,
                &test_grant(&["channels.write"], &[])
            )
            .await
            .unwrap_err()
            .code,
            "AGENT_SCOPE_DENIED"
        );
        let host = Fake::new(vec![(
            "local-data:channel-config-patch",
            json!({"apiKey":"LEAK"}),
        )]);
        let result = invoke(
            &host,
            "c",
            "channels.create",
            &params,
            &test_grant(&["channels.write", "channels.credentials.write"], &[]),
        )
        .await
        .unwrap();
        assert!(!result.to_string().contains("LEAK"));
        assert_eq!(
            host.calls.lock().unwrap()[0].1[0]["credentials"]["apiKey"],
            "LEAK"
        );
    }
    #[tokio::test]
    async fn account_is_boolean_projection_only() {
        let host = Fake::new(vec![(
            "account:status",
            json!({"sessionPresent":true,"localAllowed":true,"profile":{"email":"LEAK","everActivated":true},"error":"LEAK","cookie":"LEAK"}),
        )]);
        let result = invoke(
            &host,
            "c",
            "account.status",
            &json!({}),
            &test_grant(&["read"], &[]),
        )
        .await
        .unwrap();
        assert!(!result.to_string().contains("LEAK"));
        assert!(result.as_object().unwrap().values().all(Value::is_boolean));
    }
    #[tokio::test]
    async fn discovery_does_not_accept_untrusted_config_or_urls() {
        let host = Fake::new(vec![]);
        let result=invoke(&host,"c","channels.models",&json!({"providerId":"p","capability":"image","config":{"baseUrl":"https://attacker.test"}}),&test_grant(&["channels.read"],&[])).await;
        assert!(result.is_err());
        assert_eq!(host.calls.lock().unwrap().len(), 0);
    }
    #[tokio::test]
    async fn history_ownership_not_caller_hint_controls_retry() {
        let host = Fake::new(vec![
            ("local-data:tasks-list", json!([])),
            (
                "task-history:detail",
                json!({"task":{"id":"t","targetDraftId":"other"}}),
            ),
        ]);
        assert_eq!(
            invoke(
                &host,
                "c",
                "task.retry",
                &json!({"taskId":"t","draftId":"mine"}),
                &test_grant(&["generate"], &["mine"])
            )
            .await
            .unwrap_err()
            .code,
            "AGENT_DRAFT_DENIED"
        );
    }
    #[tokio::test]
    async fn global_action_cannot_borrow_a_scoped_draft() {
        let host = Fake::new(vec![]);
        assert_eq!(invoke(&host,"c","task.intent",&json!({"action":"modelConnectionTest","requestId":"r","target":{"draftId":"mine"}}),&test_grant(&["generate"],&["mine"])).await.unwrap_err().code,"AGENT_DRAFT_DENIED");
    }
    #[tokio::test]
    async fn ipc_error_never_echoes_provider_failure_or_secret() {
        let host = Fake::new(vec![(
            "account:status",
            json!({"__aistudioIpcError":1,"error":{"code":"PROVIDER_FAILED","message":"LEAK","details":{"apiKey":"LEAK"}}}),
        )]);
        let error = invoke(
            &host,
            "c",
            "account.status",
            &json!({}),
            &test_grant(&["read"], &[]),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PROVIDER_FAILED");
        assert!(!serde_json::to_string(&error).unwrap().contains("LEAK"));
    }
    #[test]
    fn plugin_schema_keeps_write_only_field_metadata_not_values() {
        let v = plugin_view(
            &json!({"id":"p","entry":"LEAK","installPath":"LEAK","configSchema":{"type":"object","properties":{"apiKey":{"type":"string","title":"API key","secret":true,"default":"LEAK"},"model":{"type":"string","default":"public-model"}}}}),
        );
        assert!(!v.to_string().contains("LEAK"));
        assert_eq!(v["configSchema"]["properties"]["apiKey"]["writeOnly"], true);
        assert_eq!(
            v["configSchema"]["properties"]["model"]["default"],
            "public-model"
        );
    }
    #[tokio::test]
    async fn all_registered_intents_use_the_existing_intent_channel() {
        for action in ACTIONS {
            let mut responses = vec![];
            if !global_action(action) {
                responses.push(("local-data:drafts-get", json!({"id":"d"})));
            }
            responses.push(("ai-task:intent",json!({"requestId":"r","tasks":[{"id":"t","targetDraftId":"d","inputSnapshot":{"apiKey":"LEAK"}}]})));
            let host = Fake::new(responses);
            let request =
                json!({"requestId":"r","action":action,"target":{"draftId":"d"},"options":{}});
            let result = invoke(
                &host,
                "c",
                "task.intent",
                &request,
                &test_grant(&["generate", "export"], &["*"]),
            )
            .await
            .unwrap();
            assert_eq!(result["tasks"][0]["id"], "t");
            assert!(!result.to_string().contains("LEAK"));
            assert_eq!(host.calls.lock().unwrap().last().unwrap().1[0], request);
        }
    }
    #[tokio::test]
    async fn batch_uses_real_run_id_not_a_task_style_id() {
        let host = Fake::new(vec![
            ("batch:get", json!({"runId":"run","draftId":"d"})),
            ("batch:pause", json!({"runId":"run","status":"paused"})),
        ]);
        let result = invoke(
            &host,
            "c",
            "batch.pause",
            &json!({"draftId":"d","runId":"run"}),
            &test_grant(&["edit"], &["d"]),
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "paused");
        assert_eq!(host.calls.lock().unwrap()[1].1, vec![json!("run")]);
    }
    #[tokio::test]
    async fn export_retry_requires_export_permission() {
        let host = Fake::new(vec![(
            "local-data:tasks-list",
            json!([{"id":"t","type":"export","targetDraftId":"d"}]),
        )]);
        assert_eq!(
            invoke(
                &host,
                "c",
                "task.retry",
                &json!({"taskId":"t"}),
                &test_grant(&["generate"], &["d"])
            )
            .await
            .unwrap_err()
            .code,
            "AGENT_SCOPE_DENIED"
        );
        assert_eq!(host.calls.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn workflow_resume_checks_actual_owner() {
        let host = Fake::new(vec![("workflow:get", json!({"id":"w","draftId":"other"}))]);
        assert_eq!(
            invoke(
                &host,
                "c",
                "workflow.resume",
                &json!({"workflowId":"w","stepKey":"video","draftId":"d"}),
                &test_grant(&["generate"], &["d"])
            )
            .await
            .unwrap_err()
            .code,
            "AGENT_DRAFT_DENIED"
        );
    }
    #[test]
    fn task_results_are_locatable_without_raw_provider_errors() {
        let task = task_view(
            &json!({"id":"t","targetShotId":"shot","resultUrl":"media://result","outputSnapshot":{"localUrl":"media://local","apiKey":"LEAK"},"error":{"code":"CONTENT_REJECTED","message":"LEAK","class":"terminal","retryable":false}}),
        );
        assert_eq!(task["resultUrl"], "media://result");
        assert_eq!(task["outputSnapshot"]["localUrl"], "media://local");
        assert_eq!(task["error"]["code"], "CONTENT_REJECTED");
        assert!(!task.to_string().contains("LEAK"));
    }
    #[test]
    fn plugin_presets_obey_manifest_secret_fields() {
        let plugin = plugin_view(
            &json!({"id":"p","configSchema":{"type":"object","properties":{"unusualName":{"type":"string","secret":true},"model":{"type":"string"}}},"channelPresets":[{"id":"a","name":"A","config":{"unusualName":"LEAK","model":"visible"}}]}),
        );
        assert!(!plugin.to_string().contains("LEAK"));
        assert_eq!(plugin["channelPresets"][0]["config"]["model"], "visible");
    }
    #[tokio::test]
    async fn global_workflow_cannot_borrow_scoped_draft() {
        for kind in ["videoCaseAnalysis", "derivedDraftFromCase"] {
            let host = Fake::new(vec![]);
            assert_eq!(invoke(&host,"c","workflow.start",&json!({"kind":kind,"target":{"draftId":"d","entity":"videoCase","ids":["case"]}}),&test_grant(&["generate"],&["d"])).await.unwrap_err().code,"AGENT_DRAFT_DENIED");
        }
    }
    #[test]
    fn orchestration_never_echoes_nested_provider_error_strings() {
        let value = orchestration_view(
            &json!({"runId":"r","draftId":"d","blockedReason":{"message":"LEAK","code":"CONTENT_REJECTED"},"options":{"prompt":"LEAK"},"episodes":{"e":{"stages":{"s":{"error":"LEAK","done":1}}}}}),
        );
        assert!(!value.to_string().contains("LEAK"));
        assert_eq!(value["episodes"]["e"]["stages"]["s"]["done"], 1);
    }
    #[tokio::test]
    async fn workflow_seed_tasks_must_belong_to_granted_draft() {
        let host = Fake::new(vec![(
            "local-data:tasks-list",
            json!([{"id":"t","targetDraftId":"other"}]),
        )]);
        assert_eq!(invoke(&host,"c","workflow.start",&json!({"kind":"shotRegenerate","target":{"draftId":"d"},"options":{"seedTaskIds":["t"]}}),&test_grant(&["generate"],&["d"])).await.unwrap_err().code,"AGENT_DRAFT_DENIED");
    }
    #[test]
    fn write_only_schema_strips_nested_defaults_enums_and_presets() {
        let schema = json!({"type":"object","properties":{
            "opaque":{"type":"string","writeOnly":true,"default":"LEAK","enum":["LEAK"],"optionsSource":{"action":"LEAK"}},
            "rows":{"type":"array","default":[{"value":"LEAK","label":"public"}],"items":{"type":"object","properties":{"value":{"type":"string","writeOnly":true,"default":"LEAK"},"label":{"type":"string","default":"public"}}}},
            "hiddenRows":{"type":"array","writeOnly":true,"default":["LEAK"],"items":{"type":"string","default":"LEAK","optionsSource":{"action":"LEAK"}}}
        }});
        let projected = schema_view(&schema);
        assert!(!projected.to_string().contains("LEAK"));
        assert_eq!(projected["properties"]["opaque"]["writeOnly"], true);
        assert_eq!(
            projected["properties"]["rows"]["items"]["properties"]["value"]["writeOnly"],
            true
        );
        assert_eq!(
            projected["properties"]["hiddenRows"]["items"]["writeOnly"],
            true
        );
        assert_eq!(
            projected["properties"]["rows"]["default"][0]["label"],
            "public"
        );
        let plugin = plugin_view(
            &json!({"id":"p","configSchema":schema,"channelPresets":[{"id":"a","config":{"opaque":"LEAK","rows":[{"value":"LEAK","label":"public"}],"hiddenRows":["LEAK"]}}]}),
        );
        assert!(!plugin.to_string().contains("LEAK"));
    }
    #[tokio::test]
    async fn channel_cas_version_is_projected_forwarded_and_safely_returned() {
        let host = Fake::new(vec![
            (
                "local-data:channel-config-list",
                json!({"providers":[{"id":"p"}],"version":7}),
            ),
            (
                "local-data:channel-config-list",
                json!({"providers":[{"id":"p"}],"version":7}),
            ),
            (
                "local-data:channel-config-patch",
                json!({"version":8,"credentials":{"apiKey":"LEAK"}}),
            ),
        ]);
        let grant = test_grant(&["channels.read", "channels.write"], &[]);
        assert_eq!(
            invoke(&host, "c", "channels.list", &json!({}), &grant)
                .await
                .unwrap()[0]["configurationVersion"],
            7
        );
        assert_eq!(
            invoke(
                &host,
                "c",
                "channels.get",
                &json!({"providerId":"p"}),
                &grant
            )
            .await
            .unwrap()["configurationVersion"],
            7
        );
        let result = invoke(
            &host,
            "c",
            "channels.update",
            &json!({"providerId":"p","patch":{"name":"Updated"},"expectedVersion":7}),
            &grant,
        )
        .await
        .unwrap();
        assert_eq!(host.calls.lock().unwrap()[2].1[0]["expectedVersion"], 7);
        assert_eq!(result, json!({"success":true,"providerId":"p","version":8}));
        assert!(!result.to_string().contains("LEAK"));
    }
}

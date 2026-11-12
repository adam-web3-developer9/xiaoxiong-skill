mod library;
mod agent_channels;
use crate::*;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
pub(crate) fn raw(db: &Connection, namespace: &str, key: &str) -> Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT value_json FROM kv_store WHERE namespace=? AND key=?",
            [namespace, key],
            |r| r.get(0),
        )
        .optional()?)
}
pub(crate) fn get(db: &Connection, namespace: &str, key: &str) -> Result<Value> {
    raw(db, namespace, key)?
        .map(|v| serde_json::from_str(&v).map_err(Into::into))
        .unwrap_or(Ok(Value::Null))
}
pub(crate) fn set(
    db: &Connection,
    namespace: &str,
    key: &str,
    value: &Value,
    now: i64,
) -> Result<()> {
    db.execute("INSERT INTO kv_store(namespace,key,value_json,updated_at) VALUES(?,?,?,?) ON CONFLICT(namespace,key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",params![namespace,key,json_stringify(value),now])?;
    Ok(())
}
fn allowed(namespace: &str, key: &str) -> Result<()> {
    if namespace == "image-editor" && key == "projects-v1" {
        return Err(Error {
            code: "FIELD_REJECTED".into(),
            message: "Image editor projects must be changed through data:apply.".into(),
            details: json!({"namespace":namespace,"key":key}),
        });
    }
    Ok(())
}
pub(crate) fn version(db: &Connection) -> Result<i64> {
    Ok(aistudio_config_domain::integer(
        &get(db, "runtime", "settings-version")?,
        0,
    ))
}
pub(crate) fn advance(db: &Connection, now: i64, events: &mut Vec<Value>) -> Result<i64> {
    let version = version(db)? + 1;
    set(db, "runtime", "settings-version", &json!(version), now)?;
    events.push(json!({"entity":"settings","version":version}));
    Ok(version)
}
fn snapshot(db: &Connection, ctx: &ConfigContext) -> Result<Value> {
    let raw = raw(db, "settings", "app")?.unwrap_or("{}".into());
    let rows = secrets::rows_for_context(db,ctx)?;
    let mut hash = Sha256::new();
    hash.update(raw.as_bytes());
    hash.update(serde_json::to_vec(&rows)?);
    Ok(
        json!({"storage":if ctx.cipher.local_storage(){"local-config-v1"}else{"os-vault"},"settings":serde_json::from_str::<Value>(&raw)?,"version":version(db)?,"rows":rows,"digest":format!("{:x}",hash.finalize())}),
    )
}
fn externalize(value: &Value, key_hint: &str, ctx: &ConfigContext) -> Result<Value> {
    if let Some(source) = value.as_str().filter(|s| s.starts_with("data:")) {
        if let Some(media) = &ctx.media {
            if let Some(url) = media.ingest_data_url(source, key_hint)? {
                return Ok(json!(url));
            }
        } else if source.contains(',') && !source.contains(['\r', '\n']) {
            return Err(error(
                "CONFIG_MEDIA_CONTEXT_MISSING",
                "Native config media storage is unavailable",
            ));
        }
    }
    match value {
        Value::Array(items) => Ok(json!(items
            .iter()
            .enumerate()
            .map(|(index, value)| externalize(value, &format!("{key_hint}-{index}"), ctx))
            .collect::<Result<Vec<_>>>()?)),
        Value::Object(fields) => {
            let mut result = json!({});
            for (key, value) in fields {
                result[key] = externalize(value, key, ctx)?;
            }
            Ok(result)
        }
        _ => Ok(value.clone()),
    }
}
pub(crate) fn dispatch(
    db: &Connection,
    op: &str,
    a: &Value,
    now: i64,
    ctx: &ConfigContext,
    events: &mut Vec<Value>,
) -> Result<Value> {
    let namespace = text(&a["namespace"]);
    let key = text(&a["key"]);
    match op {
        "config.channels.list" | "config.channels.patch" | "config.channels.optionsContext" | "config.styles.referencesPatch" => agent_channels::dispatch(db, op, a, now, ctx, events),
        "config.library.snapshot" | "config.library.apply" => library::dispatch(db, op, a, now, ctx, events),
        "config.kv.get" => get(db, &namespace, &key),
        "config.kv.updatedAt" => Ok(db.query_row(
            "SELECT updated_at FROM kv_store WHERE namespace=? AND key=?",
            [&namespace, &key], |row| row.get::<_, i64>(0),
        ).optional()?.map(|value|json!(value)).unwrap_or(Value::Null)),
        "config.kv.getRecord" => {
            let row = db
                .query_row(
                    "SELECT value_json,updated_at FROM kv_store WHERE namespace=? AND key=?",
                    [&namespace, &key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            Ok(if let Some((value, updated)) = row {
                json!({"value":serde_json::from_str::<Value>(&value)?,"updatedAt":updated})
            } else {
                Value::Null
            })
        }
        "config.kv.list" => {
            let mut statement =
                db.prepare("SELECT key,value_json,updated_at FROM kv_store WHERE namespace=?")?;
            let rows = statement
                .query_map([&namespace], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(json!(rows.into_iter().map(|(key,value,updated)|Ok(json!({"key":key,"value":serde_json::from_str::<Value>(&value)?,"updatedAt":updated}))).collect::<Result<Vec<_>>>()?))
        }
        "config.media.externalize" => externalize(&a["value"], &or_text(&a["keyHint"], ""), ctx),
        "config.kv.set" => {
            allowed(&namespace, &key)?;
            let app = namespace == "settings" && key == "app";
            let value = if app {
                aistudio_config_domain::canonical_settings_record(a["value"].clone())?
            } else {
                a["value"].clone()
            };
            let value = externalize(&value, &format!("{namespace}-{key}"), ctx)?;
            set(db, &namespace, &key, &value, now)?;
            let version = if app {
                json!(advance(db, now, events)?)
            } else {
                Value::Null
            };
            Ok(json!({"success":true,"version":version}))
        }
        "config.kv.delete" => {
            allowed(&namespace, &key)?;
            db.execute(
                "DELETE FROM kv_store WHERE namespace=? AND key=?",
                [&namespace, &key],
            )?;
            if namespace == "settings" && key == "app" {
                advance(db, now, events)?;
            }
            Ok(json!({"success":true}))
        }
        "config.settings.snapshot" => snapshot(db,ctx),
        "config.maintenance.externalizeMedia" => {
            if raw(db, "runtime", "kv-media-externalized-v1")?.is_some() {
                return Ok(json!({"alreadyComplete":true}));
            }
            let mut statement=db.prepare("SELECT namespace,key,value_json FROM kv_store WHERE namespace IN ('settings','settings_backup') AND instr(value_json,'data:')>0")?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut changed = 0;
            for (namespace, key, raw) in rows {
                let value: Value = serde_json::from_str(&raw)?;
                if value.is_null() {
                    continue;
                }
                let value = externalize(&value, &format!("{namespace}-{key}"), ctx)?;
                set(db, &namespace, &key, &value, now)?;
                changed += 1;
            }
            set(db, "runtime", "kv-media-externalized-v1", &json!(true), now)?;
            Ok(json!({"changed":changed}))
        }
        "config.settings.ensureVoiceDemo" => {
            let current = get(db, "settings", "app")?;
            let mut settings = if current.is_object() {
                current
            } else {
                json!({})
            };
            if !truth(&settings["voice"]) || settings["voice"].get("designDemoText").is_none() {
                let mut voice = object(&settings["voice"]);
                voice["designDemoText"] =
                    aistudio_config_domain::registry()["voiceDesignDemoText"].clone();
                settings["voice"] = voice;
                set(db, "settings", "app", &settings, now)?;
                return Ok(json!({"changed":true}));
            }
            Ok(json!({"changed":false}))
        }
        "config.settings.commit" => {
            let saved = snapshot(db,ctx)?;
            if a["expectedVersion"] != saved["version"] || a["expectedDigest"] != saved["digest"] {
                return Err(error(
                    "SETTINGS_TRANSFER_CONFLICT",
                    "本机配置或密钥在预览后发生变化，请重新预览",
                ));
            }
            if a["changed"] == false
                && array(&a["secretWrites"]).is_empty()
                && array(&a["secretRows"]).is_empty()
            {
                return Ok(json!({"success":true,"version":saved["version"],"changed":false}));
            }
            secrets::write(db, &a["secretWrites"], now, ctx, events)?;
            secrets::persist(db, &array(&a["secretRows"]), now)?;
            for row in array(&a["secretRows"]) {
                events.push(json!({"entity":"secret","key":row["key"]}));
            }
            set(db, "settings", "app", &a["settings"], now)?;
            Ok(json!({"success":true,"version":advance(db,now,events)?,"changed":true}))
        }
        "config.settings.canonicalize" => {
            let Some(raw) = raw(db, "settings", "app")? else {
                return Ok(json!({"migrated":false,"version":null}));
            };
            let canonical =
                aistudio_config_domain::canonical_settings_record(serde_json::from_str(&raw)?)?;
            if json_stringify(&canonical) == raw {
                return Ok(json!({"migrated":false,"version":null}));
            }
            set(db, "settings", "app", &canonical, now)?;
            Ok(json!({"migrated":true,"version":advance(db,now,events)?}))
        }
        "config.logs.upsert" => {
            let entry = &a["entry"];
            let id = or_text(&entry["id"], "");
            if id.is_empty() {
                return Err(error(
                    "CONFIG_INVALID_ARGUMENT",
                    "Execution log id is required",
                ));
            }
            let timestamp = entry["timestamp"]
                .as_i64()
                .filter(|n| *n != 0)
                .unwrap_or(now);
            db.execute("INSERT INTO execution_logs(id,data_json,timestamp) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET data_json=excluded.data_json,timestamp=excluded.timestamp",params![id,json_stringify(entry),timestamp])?;
            Ok(json!({"success":true}))
        }
        "config.logs.clear" => {
            db.execute("DELETE FROM execution_logs", [])?;
            Ok(json!({"success":true}))
        }
        "config.secrets.migrateSettings" => {
            let settings = get(db, "settings", "app")?;
            if !settings.is_object() {
                return Ok(json!({"migrated":0,"skipped":""}));
            }
            let writes = aistudio_config_domain::collect_secret_writes(&settings)
                .into_iter()
                .filter(|w| w.get("value").is_some())
                .collect::<Vec<_>>();
            if writes.is_empty() {
                return Ok(json!({"migrated":0,"skipped":""}));
            }
            let mut missing = Vec::new();
            for write in &writes {
                if ctx.cipher.local_storage() && db.query_row("SELECT EXISTS(SELECT 1 FROM local_interface_values WHERE key=?)",[text(&write["ref"])],|r|r.get::<_,bool>(0))? {continue;}
                if !truth(&secrets::get(db, &text(&write["ref"]), ctx)?) {
                    missing.push(write.clone());
                }
            }
            secrets::write(db, &json!(missing), now, ctx, events)?;
            set(
                db,
                "settings",
                "app",
                &aistudio_config_domain::strip_secret_inputs(&settings),
                now,
            )?;
            Ok(json!({"migrated":writes.len(),"skipped":""}))
        }
        _ => Err(error(
            "CONFIG_OPERATION_UNKNOWN",
            format!("Unknown config operation: {op}"),
        )),
    }
}


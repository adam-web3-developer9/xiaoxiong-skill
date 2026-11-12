use serde_json::{json, Value};
pub(super) fn sensitive_values(value: &Value, schema: &Value, out: &mut Vec<String>) {
    if schema["secret"] == true || schema["writeOnly"] == true {
        collect_strings(value, out);
    } else if let Some(fields) = schema["properties"].as_object() {
        for (key, field) in fields {
            sensitive_values(&value[key], field, out);
            if (field["secret"] == true || field["writeOnly"] == true)
                && field["legacyField"].is_string()
            {
                collect_strings(&value[field["legacyField"].as_str().unwrap()], out);
            }
        }
    } else if let Some(values) = value.as_array() {
        for value in values {
            sensitive_values(value, &schema["items"], out);
        }
    }
}
pub(super) fn collect_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(v) if !v.is_empty() => out.push(v.clone()),
        Value::Array(values) => {
            for value in values {
                collect_strings(value, out);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_strings(value, out);
            }
        }
        _ => {}
    }
}
pub(super) fn encoded_secret(value: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(value)
}
pub(super) fn secret_names(schema: &Value, out: &mut Vec<String>) {
    if let Some(fields) = schema["properties"].as_object() {
        for (key, field) in fields {
            if field["secret"] == true || field["writeOnly"] == true {
                out.push(key.to_ascii_lowercase().replace(['_', '-'], ""));
                if let Some(legacy) = field["legacyField"].as_str() {
                    out.push(legacy.to_ascii_lowercase().replace(['_', '-'], ""));
                }
            }
            secret_names(field, out);
        }
    }
    if let Some(items) = schema.get("items") {
        secret_names(items, out);
    }
}
pub(super) fn public_result(value: &Value, secrets: &[String], private_names: &[String]) -> Value {
    match value {
        Value::String(value) => {
            let mut value = value.clone();
            for secret in secrets {
                value = value.replace(secret, "[redacted]");
            }
            Value::String(value)
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|v| public_result(v, secrets, private_names))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .filter(|(key, _)| {
                    let key = key.to_ascii_lowercase().replace(['_', '-'], "");
                    !private_names.contains(&key)
                        && ![
                            "apikey",
                            "privatekey",
                            "secret",
                            "token",
                            "cookie",
                            "password",
                            "authorization",
                            "credential",
                            "fingerprint",
                            "valuebase64",
                        ]
                        .iter()
                        .any(|part| key.contains(part))
                })
                .map(|(key, value)| {
                    (
                        public_result(&json!(key), secrets, private_names)
                            .as_str()
                            .unwrap()
                            .to_owned(),
                        public_result(value, secrets, private_names),
                    )
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dynamic_options_preserve_ui_sections_but_remove_credential_literals() {
        let value = json!({"options":[{"value":"item","label":"model sk-sample","details":{"name":"sk-sample"},"updates":{"quality":"hd","apiKey":"sk-sample"},"features":[{"help":"okay"}],"stats":{"help":"okay"}}],"selection":{"webLoginError":"sk-sample"}});
        let safe = public_result(&value, &["sk-sample".into()], &[]);
        assert!(!safe.to_string().contains("sk-sample"));
        assert!(safe["options"][0]["updates"].get("apiKey").is_none());
        assert_eq!(safe["options"][0]["features"][0]["help"], "okay");
        assert_eq!(safe["selection"]["webLoginError"], "[redacted]");
        assert_eq!(encoded_secret("hello"), "aGVsbG8=");
    }
}
#[cfg(test)]
mod schema_tests {
    use super::*;
    #[test]
    fn dynamic_unknown_secret_field_and_legacy_alias_are_hidden() {
        let schema = json!({"type":"object","properties":{"opaque":{"type":"string","secret":true,"legacyField":"oldOpaque"}}});
        let mut secrets = vec![];
        sensitive_values(
            &json!({"oldOpaque":"private-literal"}),
            &schema,
            &mut secrets,
        );
        let mut names = vec![];
        secret_names(&schema, &mut names);
        let result = public_result(
            &json!({"label":"private-literal","updates":{"opaque":"new-secret","oldOpaque":"old-secret","quality":"hd"}}),
            &secrets,
            &names,
        );
        assert_eq!(result["label"], "[redacted]");
        assert_eq!(result["updates"], json!({"quality":"hd"}));
    }
}

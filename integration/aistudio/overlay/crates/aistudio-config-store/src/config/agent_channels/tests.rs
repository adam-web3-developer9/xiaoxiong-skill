use super::*;
struct LocalCipher;
impl secrets::SecretCipher for LocalCipher {
    fn available(&self) -> bool {
        true
    }
    fn local_storage(&self) -> bool {
        true
    }
    fn encrypt(&self, _: &str) -> Result<Vec<u8>> {
        panic!("local config must not encrypt")
    }
    fn decrypt(&self, _: &[u8]) -> Result<String> {
        panic!("presence projection must not decrypt")
    }
}
fn context() -> ConfigContext {
    ConfigContext {
        cipher: std::sync::Arc::new(LocalCipher),
        ..Default::default()
    }
}
fn manifests() -> Value {
    json!([{"id":"plugin","version":"1","packageDigest":"digest","capabilities":{"image":true,"text":true},"configSchema":{"type":"object","properties":{
        "baseUrl":{"type":"string"},"imageModel":{"type":"string","optionsSource":{"action":"models"}},
        "apiKey":{"type":"string","secret":true},"secondaryApiKey":{"type":"string","secret":true},
        "custom":{"type":"object","properties":{"region":{"type":"string"},"privateValue":{"type":"string","secret":true}}},
        "opaqueEndpoint":{"type":"string"},"hidden":{"type":"string","writeOnly":true},
        "secretList":{"type":"array","items":{"type":"string","secret":true}}
    }}}])
}
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    crate::ensure(&db).unwrap();
    set(&db, "settings", "app", &json!({"untouched":{"value":"keep"},"aiRouting":{"providers":[]},"globalStyleLibrary":{"styles":[{"id":"style","refImages":["old"],"prompt":"keep prompt"}]}}), 1).unwrap();
    db
}
fn call(db: &mut Connection, op: &str, mut args: Value) -> Result<Value> {
    args["manifests"] = manifests();
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let value = dispatch(&tx, op, &args, 2, &context(), &mut Vec::new())?;
    tx.commit()?;
    Ok(value)
}
fn create(db: &mut Connection) {
    call(db, "config.channels.patch", json!({"providerId":"p","create":true,"patch":{"name":"before","channelPluginId":"plugin","baseUrl":"https://good.test","imageModel":"one","custom":{"region":"north"}},"credentials":{"apiKey":"primary-private","secondaryApiKey":"secondary-private","custom":{"privateValue":"custom-private"}}})).unwrap();
}
#[test]
fn create_and_public_list_never_reveal_values_references_or_rows() {
    let mut db = database();
    create(&mut db);
    let result = call(&mut db, "config.channels.list", json!({})).unwrap();
    let provider = &result["providers"][0];
    assert_eq!(provider["credentialsConfigured"]["primary"], true);
    assert_eq!(provider["credentialsConfigured"]["secondary"], true);
    assert_eq!(provider["custom"]["region"], "north");
    assert_eq!(
        provider["credentialsConfigured"]["plugin"]["/custom/privateValue"],
        true
    );
    for forbidden in [
        "primary-private",
        "secondary-private",
        "custom-private",
        "secretRef",
        "value_base64",
        "rows",
        "digest",
    ] {
        assert!(!result.to_string().contains(forbidden), "{forbidden}");
    }
    assert_eq!(
        secrets::get(&db, "direct-provider:p:apiKey", &context()).unwrap(),
        "primary-private"
    );
}
#[test]
fn non_secret_patches_preserve_credentials_and_unrelated_settings() {
    let mut db = database();
    create(&mut db);
    call(
        &mut db,
        "config.channels.patch",
        json!({"providerId":"p","patch":{"name":"renamed"}}),
    )
    .unwrap();
    call(
        &mut db,
        "config.channels.patch",
        json!({"providerId":"p","patch":{"imageModel":"two","enabled":false}}),
    )
    .unwrap();
    let saved = get(&db, "settings", "app").unwrap();
    assert_eq!(saved["aiRouting"]["providers"][0]["name"], "renamed");
    assert_eq!(
        saved["aiRouting"]["providers"][0]["custom"]["privateValue"],
        "custom-private"
    );
    assert_eq!(saved["untouched"]["value"], "keep");
    assert_eq!(
        secrets::get(&db, "direct-provider:p:apiKey", &context()).unwrap(),
        "primary-private"
    );
}
#[test]
fn explicit_clear_and_replacement_are_atomic() {
    let mut db = database();
    create(&mut db);
    db.execute_batch("CREATE TRIGGER fail_settings BEFORE UPDATE ON kv_store WHEN NEW.namespace='settings' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    let fail = call(
        &mut db,
        "config.channels.patch",
        json!({"providerId":"p","patch":{"name":"must rollback"},"credentials":{"apiKey":"new-private","secondaryApiKey":null}}),
    );
    assert!(fail.is_err());
    assert_eq!(
        secrets::get(&db, "direct-provider:p:apiKey", &context()).unwrap(),
        "primary-private"
    );
    assert_eq!(
        secrets::get(&db, "direct-provider:p:secondaryApiKey", &context()).unwrap(),
        "secondary-private"
    );
    db.execute_batch("DROP TRIGGER fail_settings").unwrap();
    call(&mut db, "config.channels.patch", json!({"providerId":"p","patch":{},"credentials":{"apiKey":"new-private","secondaryApiKey":null}})).unwrap();
    assert_eq!(
        secrets::get(&db, "direct-provider:p:apiKey", &context()).unwrap(),
        "new-private"
    );
    assert!(
        secrets::get(&db, "direct-provider:p:secondaryApiKey", &context())
            .unwrap()
            .is_null()
    );
}
#[test]
fn endpoint_plugin_and_new_inherited_capability_changes_cannot_reuse_old_secrets() {
    let mut db = database();
    create(&mut db);
    for patch in [
        json!({"baseUrl":"https://evil.test"}),
        json!({"channelPluginBindings":{"image":"other"}}),
        json!({"opaqueEndpoint":"https://evil.test"}),
        json!({"channelPluginCapabilityConfigs":{"image":{"baseUrl":"https://evil.test","inheritCredentials":true}}}),
    ] {
        let err = call(
            &mut db,
            "config.channels.patch",
            json!({"providerId":"p","patch":patch}),
        )
        .unwrap_err();
        assert_eq!(err.code, "CHANNEL_CREDENTIAL_DESTINATION_CHANGED");
    }
    call(&mut db, "config.channels.patch", json!({"providerId":"p","patch":{"baseUrl":"https://new.test"},"credentials":{"apiKey":"new-primary","secondaryApiKey":null,"custom":{"privateValue":null}}})).unwrap();
    assert_eq!(
        get(&db, "settings", "app").unwrap()["aiRouting"]["providers"][0]["baseUrl"],
        "https://new.test"
    );
}
#[test]
fn public_patch_cannot_write_secret_fields_even_nested_array_schemas() {
    let mut db = database();
    create(&mut db);
    for patch in [
        json!({"apiKey":"bad"}),
        json!({"secretRef":"auth:token"}),
        json!({"custom":{"privateValue":"bad"}}),
        json!({"hidden":"bad"}),
        json!({"secretList":["bad"]}),
    ] {
        assert_eq!(
            call(
                &mut db,
                "config.channels.patch",
                json!({"providerId":"p","patch":patch})
            )
            .unwrap_err()
            .code,
            "CHANNEL_CONFIG_INVALID"
        );
    }
}
#[test]
fn style_reference_patch_never_roundtrips_masked_settings_or_touches_credentials() {
    let mut db = database();
    create(&mut db);
    call(
        &mut db,
        "config.styles.referencesPatch",
        json!({"styleId":"style","refImages":["aistudio://media/ref"]}),
    )
    .unwrap();
    let saved = get(&db, "settings", "app").unwrap();
    assert_eq!(
        saved["globalStyleLibrary"]["styles"][0]["refImages"],
        json!(["aistudio://media/ref"])
    );
    assert_eq!(
        saved["globalStyleLibrary"]["styles"][0]["prompt"],
        "keep prompt"
    );
    assert_eq!(
        secrets::get(&db, "direct-provider:p:apiKey", &context()).unwrap(),
        "primary-private"
    );
    assert_eq!(
        saved["aiRouting"]["providers"][0]["custom"]["privateValue"],
        "custom-private"
    );
}
#[test]
fn dynamic_context_uses_saved_connection_and_only_declared_actions() {
    let mut db = database();
    create(&mut db);
    let mut args = json!({"providerId":"p","capability":"image","action":"models","config":{"baseUrl":"https://evil.test","secretRef":"auth:token"}});
    let context = call(&mut db, "config.channels.optionsContext", args.clone()).unwrap();
    assert_eq!(context["config"]["baseUrl"], "https://good.test");
    assert_eq!(context["config"]["apiKey"], "primary-private");
    assert!(context["config"].get("secretRef").is_none());
    assert!(context["config"]
        .get("channelPluginCapabilityConfigs")
        .is_none());
    args["action"] = json!("arbitrary-exfil");
    assert_eq!(
        call(&mut db, "config.channels.optionsContext", args)
            .unwrap_err()
            .code,
        "PLUGIN_OPTIONS_ACTION_UNKNOWN"
    );
}
#[test]
fn stale_version_rejects_without_modifying_config_or_credentials() {
    let mut db = database();
    create(&mut db);
    let saved = get(&db, "settings", "app").unwrap();
    assert_eq!(call(&mut db,"config.channels.patch",json!({"providerId":"p","expectedVersion":0,"patch":{"name":"lost"},"credentials":{"apiKey":"bad"}})).unwrap_err().code,"CHANNEL_CONFIG_CONFLICT");
    assert_eq!(get(&db, "settings", "app").unwrap(), saved);
    assert_eq!(
        secrets::get(&db, "direct-provider:p:apiKey", &context()).unwrap(),
        "primary-private"
    );
}
#[test]
fn independent_owner_transactions_merge_current_fields_without_lost_updates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.sqlite");
    let mut first = Connection::open(&path).unwrap();
    crate::ensure(&first).unwrap();
    create(&mut first);
    let ready = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = [json!({"name":"concurrent-name"}), json!({"enabled":false})]
        .into_iter()
        .map(|patch| {
            let path = path.clone();
            let ready = ready.clone();
            std::thread::spawn(move || {
                let mut connection = Connection::open(path).unwrap();
                connection
                    .busy_timeout(std::time::Duration::from_secs(2))
                    .unwrap();
                ready.wait();
                call(
                    &mut connection,
                    "config.channels.patch",
                    json!({"providerId":"p","patch":patch}),
                )
                .unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let saved = get(&first, "settings", "app").unwrap();
    assert_eq!(
        saved["aiRouting"]["providers"][0]["name"],
        "concurrent-name"
    );
    assert_eq!(saved["aiRouting"]["providers"][0]["enabled"], false);
    assert_eq!(
        secrets::get(&first, "direct-provider:p:apiKey", &context()).unwrap(),
        "primary-private"
    );
}
#[test]
fn schema_write_only_host_named_fields_cannot_return_through_host_projection() {
    let provider = json!({"id":"p","name":"not-public","channelPluginId":"plugin","baseUrl":"https://good.test?key=secret","apiKeyConfigured":true});
    let mut manifests = manifests();
    manifests[0]["configSchema"]["properties"]["name"] = json!({"type":"string","writeOnly":true});
    let view = public_provider(&provider, &manifests);
    assert!(view.get("name").is_none());
    assert!(view.get("baseUrl").is_none());
}
#[test]
fn existing_inherited_capability_endpoint_change_requires_root_replacement() {
    let mut db = database();
    create(&mut db);
    call(&mut db,"config.channels.patch",json!({"providerId":"p","patch":{"channelPluginCapabilityConfigs":{"image":{"inheritCredentials":true}}}})).unwrap();
    let error=call(&mut db,"config.channels.patch",json!({"providerId":"p","patch":{"channelPluginCapabilityConfigs":{"image":{"baseUrl":"https://evil.test"}}}})).unwrap_err();
    assert_eq!(error.code, "CHANNEL_CREDENTIAL_DESTINATION_CHANGED");
}
#[test]
fn new_capability_custom_routing_cannot_exfiltrate_root_plugin_secrets() {
    let mut db = database();
    create(&mut db);
    for inherit in [true, false] {
        let error=call(&mut db,"config.channels.patch",json!({"providerId":"p","patch":{"channelPluginCapabilityConfigs":{"image":{"opaqueEndpoint":"https://evil.test","inheritCredentials":inherit}}}})).unwrap_err();
        assert_eq!(error.code, "CHANNEL_CREDENTIAL_DESTINATION_CHANGED");
    }
}
#[test]
fn nested_host_metadata_cannot_restore_schema_secret_values() {
    let mut manifests = manifests();
    manifests[0]["configSchema"]["properties"]["capabilities"] = json!({"type":"object","properties":{"safe":{"type":"boolean"},"hidden":{"type":"string","writeOnly":true}}});
    let result = public_provider(
        &json!({"id":"p","channelPluginId":"plugin","capabilities":{"safe":true,"hidden":"do-not-return"}}),
        &manifests,
    );
    assert_eq!(result["capabilities"]["safe"], true);
    assert!(result["capabilities"].get("hidden").is_none());
}
#[test]
fn plugin_legacy_secret_alias_is_replaced_or_cleared_before_endpoint_changes() {
    let schema = json!({"type":"object","properties":{"oldToken":{"type":"string"},"customToken":{"type":"string","secret":true,"legacyField":"oldToken"}}});
    let before = json!({"oldToken":"legacy-private"});
    assert!(schema_public(&before, &schema)
        .unwrap()
        .get("oldToken")
        .is_none());
    assert!(patch_public(
        &mut before.clone(),
        &json!({"oldToken":"new"}),
        &schema,
        false
    )
    .is_err());
    assert_eq!(
        require_replacement(&before, &json!({}), &schema)
            .unwrap_err()
            .code,
        "CHANNEL_CREDENTIAL_DESTINATION_CHANGED"
    );
    let retained = retained_credentials(&before, &before, &schema);
    assert_eq!(
        require_replacement(&retained, &json!({}), &schema)
            .unwrap_err()
            .code,
        "CHANNEL_CREDENTIAL_DESTINATION_CHANGED"
    );
    require_replacement(&retained, &json!({"customToken":null}), &schema).unwrap();
    let mut after = before;
    patch_credentials(
        &mut after,
        &json!({"customToken":"replacement"}),
        &schema,
        true,
    )
    .unwrap();
    assert_eq!(after["oldToken"], "");
    assert_eq!(after["customToken"], "replacement");
}
#[test]
fn schema_private_arrays_require_credential_input_and_can_be_cleared() {
    let mut db = database();
    create(&mut db);
    call(
        &mut db,
        "config.channels.patch",
        json!({"providerId":"p","patch":{},"credentials":{"secretList":["private-array-key"]}}),
    )
    .unwrap();
    let view = call(&mut db, "config.channels.list", json!({})).unwrap();
    assert!(!view.to_string().contains("private-array-key"));
    call(
        &mut db,
        "config.channels.patch",
        json!({"providerId":"p","patch":{},"credentials":{"secretList":null}}),
    )
    .unwrap();
    assert_eq!(
        get(&db, "settings", "app").unwrap()["aiRouting"]["providers"][0]["secretList"],
        json!([])
    );
}
#[test]
fn capability_null_clear_does_not_restore_root_schema_secret_at_new_destination() {
    let mut db = database();
    call(&mut db,"config.channels.patch",json!({"providerId":"p","create":true,"patch":{"channelPluginId":"plugin","baseUrl":"https://good.test"},"credentials":{"hidden":"root-private"}})).unwrap();
    call(&mut db,"config.channels.patch",json!({"providerId":"p","patch":{"channelPluginCapabilityConfigs":{"image":{"opaqueEndpoint":"https://evil.test"}}},"capabilityCredentials":{"image":{"hidden":null}}})).unwrap();
    let context = call(
        &mut db,
        "config.channels.optionsContext",
        json!({"providerId":"p","capability":"image","action":"models"}),
    )
    .unwrap();
    assert!(is_cleared(&context["config"]["hidden"]));
    assert!(!context["config"].to_string().contains("root-private"));
}

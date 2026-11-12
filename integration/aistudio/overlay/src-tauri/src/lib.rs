mod awake_clock;
mod account_service;
mod agent_service;
mod agent_authorization;
mod desktop_lifecycle;
mod desktop_power;
mod desktop_environment;
mod desktop_updates;
mod desktop_package_sharing;
mod operational_diagnostics;
mod diagnostics_export;
mod diagnostics_export_native;
mod renderer_health;
mod renderer_observer;
#[cfg(windows)]
mod renderer_observer_windows;
mod telemetry_ingress;
mod bridge;
mod data_read;
mod data_service;
mod export_host;
mod export_media;
mod export_service;
mod history_read;
mod media;
mod media_loopback;
mod media_save;
mod media_sources;
mod media_upload;
mod media_zip;
mod native;
mod native_storage;
mod power_events;
mod process_manager;
mod profile;
mod provider_service;
mod runtime_cipher;
pub mod proxy;
mod style_presets;
mod transport;
mod task_service;
mod task_updates;
mod task_chatgpt;
mod joojo_login;
mod desktop_public_media;
mod task_environment;
mod task_media;
mod task_epoch;
mod task_local;
mod task_legacy;
mod task_legacy_execution;
mod task_tools;
mod task_browser;
mod task_download;
mod build_identity;
mod desktop_service;
mod desktop_local;
mod desktop_exports;
mod desktop_os;
mod desktop_plugins;
mod task_auxiliary;
mod task_validation;
mod task_watermark;
mod task_plugins;
mod task_channel_plugins;
mod task_official_plugins;
mod task_official_image;
mod task_official_audio;
mod task_official_recovery;
mod task_official_storage;
mod task_plugin_migration;
mod renderer_flush;
mod exit_sequence;
pub mod vault;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::Manager;

// Tauri's navigation hook does not identify the requesting frame. Keep the
// plugin iframe's owned UI route available; its sandbox denies top navigation,
// while the remote origin is never granted the application's IPC capability.
fn navigation_allowed(url: &url::Url, development: bool) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if url.port().is_none()
        && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost")))
    {
        return true;
    }
    if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
        return false;
    }
    if development && url.port() == Some(17300) {
        return true;
    }
    if !url.port().is_some_and(|port| port != 0) {
        return false;
    }
    let mut segments = url.path().split('/');
    if segments.next() != Some("") || segments.next() != Some("plugins") {
        return false;
    }
    let Some(plugin_id) = segments.next() else {
        return false;
    };
    if plugin_id.is_empty()
        || plugin_id.len() > 96
        || !plugin_id.as_bytes()[0].is_ascii_alphanumeric()
        || !plugin_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        || segments.next() != Some("ui")
    {
        return false;
    }
    let mut resource = false;
    for segment in segments {
        let Ok(segment) = percent_encoding::percent_decode_str(segment).decode_utf8() else {
            return false;
        };
        if segment.is_empty()
            || matches!(segment.as_ref(), "." | "..")
            || segment.contains(['/', '\\', '\0'])
        {
            return false;
        }
        resource = true;
    }
    resource
}

fn open_new_window(
    app: &tauri::AppHandle,
    url: url::Url,
) -> tauri::webview::NewWindowResponse<tauri::Wry> {
    if matches!(url.scheme(), "http" | "https") {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            // Reuse the sole external URL validator/opener. Never log a URL,
            // since a third-party link may carry credentials or license data.
            if native::dispatch(
                app.clone(),
                "shell.open-external",
                json!({ "url": url.as_str() }),
            )
            .await
            .is_err()
            {
                eprintln!("EXTERNAL_OPEN_FAILED: 系统浏览器未能打开链接");
                let _ = native::dispatch(
                    app,
                    "dialog.error",
                    json!({
                        "title": "无法打开外部链接",
                        "content": "系统浏览器未能打开此链接。请检查链接及默认浏览器设置后重试。"
                    }),
                )
                .await;
            }
        });
    }
    // A plugin popup never becomes another application WebView or replaces main.
    tauri::webview::NewWindowResponse::Deny
}

fn runtime_configuration(
    app: &tauri::AppHandle,
) -> Result<(Value, PathBuf), Box<dyn std::error::Error>> {
    let paths = app.path();
    // 数据目录定位（docs/features/data-directory-install-choice.md）：env（开发/验收）>
    // 打包版跟随安装盘：安装目录父级 + AIStudio-data（安装器默认 D:\aistudio-app，装哪跟哪）>
    // dev 保持 %APPDATA%\aistudio。WebView cookies 用 Tauri 自己的标识，与该目录无关。
    let user_data = if let Some(explicit) = std::env::var_os("AISTUDIO_USER_DATA") {
        profile::select(&paths.config_dir()?, Some(PathBuf::from(explicit)))?
    } else if !cfg!(dev) {
        // 同级目录 <安装盘>\AIStudio-data：卸载器清空安装目录时不波及数据。
        let exe_dir = std::env::current_exe()?
            .parent()
            .ok_or("无法定位安装目录：current_exe 无父目录")?
            .to_path_buf();
        let install_root = exe_dir.parent().unwrap_or(&exe_dir).to_path_buf();
        install_root.join("AIStudio-data")
    } else {
        // Preserve Electron's package-name-based profile in dev (profile.rs probes existing data).
        profile::select(&paths.config_dir()?, None)?
    };
    let legacy_name = user_data
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| ["aistudio", "AIStudio"].contains(n))
        .unwrap_or("aistudio");
    let app_path = if cfg!(dev) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    } else {
        paths.resource_dir()?
    };
    let resources = if cfg!(dev) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")
    } else {
        paths.resource_dir()?
    };
    let home = paths.home_dir()?;
    let config = json!({
        "appPath": app_path, "resourcesPath": resources, "nativeExecutable": std::env::current_exe()?,
        "version": app.package_info().version.to_string(), "name":legacy_name, "legacyAppName":legacy_name, "isPackaged":!cfg!(dev),
        "paths": { "userData":user_data, "appData":paths.config_dir()?, "home":home,
            "downloads":paths.download_dir().unwrap_or(home.join("Downloads")),
            "documents":paths.document_dir().unwrap_or(home.join("Documents")),
            "videos":paths.video_dir().unwrap_or(home.join("Videos")),
            "pictures":paths.picture_dir().unwrap_or(home.join("Pictures")),
            "music":paths.audio_dir().unwrap_or(home.join("Music")),
            "desktop":paths.desktop_dir().unwrap_or(home.join("Desktop")),
            "temp":std::env::temp_dir(), "logs":user_data.join("logs") }
    });
    Ok((config, user_data.join("aistudio-data")))
}

fn native_startup_stage(
    native_started: std::time::Instant,
    stage: &str,
    started: std::time::Instant,
) -> Value {
    let observed = std::time::Instant::now();
    json!({"stage":stage,"action":"end",
        "durationMs":observed.duration_since(started).as_millis() as u64,
        "elapsedMs":observed.duration_since(native_started).as_millis() as u64,
        "observedAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true)})
}

pub fn run() {
    let native_started = std::time::Instant::now();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol(
            "aistudio-media",
            |context, request, responder| {
                let root = context
                    .app_handle()
                    .state::<bridge::Bridge>()
                    .data_root
                    .clone();
                let origin = context
                    .app_handle()
                    .state::<media_loopback::MediaLoopback>()
                    .origin()
                    .to_owned();
                std::thread::spawn(move || {
                    responder.respond(media::redirect_response(request, &root, &origin))
                });
            },
        )
        .invoke_handler(tauri::generate_handler![
            bridge::bridge_invoke,
            agent_authorization::agent_authorization,
            bridge::bridge_send,
            native::native_pick_file,
            media_origin
        ])
        .on_page_load(|webview,payload|{
            if let Some(lifecycle)=webview.app_handle().try_state::<desktop_lifecycle::NativeLifecycle>() {
                lifecycle.diagnostics.record("renderer_navigation",json!({"windowId":webview.label(),"action":if matches!(payload.event(),tauri::webview::PageLoadEvent::Started){"start"}else{"end"}}));
                if matches!(payload.event(),tauri::webview::PageLoadEvent::Started){lifecycle.renderer_health.navigation();}
            }
            if matches!(payload.event(),tauri::webview::PageLoadEvent::Started){
                if let Some(flush)=webview.app_handle().try_state::<renderer_flush::RendererFlush>(){flush.forget(webview.label());}
                if let Some(service)=webview.app_handle().try_state::<desktop_service::DesktopService>(){service.builds.lock().unwrap().forget(webview.label());}
            }
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed=event {
                if let Some(lifecycle)=window.app_handle().try_state::<desktop_lifecycle::NativeLifecycle>(){lifecycle.diagnostics.record("renderer_window_destroyed",json!({"windowId":window.label()}));}
                if let Some(flush)=window.app_handle().try_state::<renderer_flush::RendererFlush>(){flush.forget(window.label());}
                if let Some(service)=window.app_handle().try_state::<desktop_service::DesktopService>(){service.builds.lock().unwrap().forget(window.label());}
            }
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if !app.try_state::<desktop_lifecycle::NativeLifecycle>().is_some_and(|lifecycle|lifecycle.exit.ready()) {
                    api.prevent_close();
                    let app=app.clone();tauri::async_runtime::spawn(async move{
                        if let Some(lifecycle)=app.try_state::<desktop_lifecycle::NativeLifecycle>(){
                            if let Err(error)=lifecycle.close_window().await{eprintln!("Native window close failed: {}",error["code"]);}
                        }else{app.exit(0);}
                    });
                }
            }
        })
        .setup(move |app| {
            // Buffer only timing observations until the shared diagnostic owner
            // exists. No second writer is opened during native initialization.
            let mut startup_stages = Vec::new();
            let configuration_started = std::time::Instant::now();
            app.manage(renderer_flush::RendererFlush::default());
            let (config, root) = runtime_configuration(app.handle())?;
            startup_stages.push(native_startup_stage(native_started,"native.runtime_configuration",configuration_started));
            let window_started = std::time::Instant::now();
            style_presets::seed(
                std::path::Path::new(config["resourcesPath"].as_str().unwrap_or("")),
                &root,
            );
            app.manage(media_loopback::MediaLoopback::start(root.clone())?);
            app.manage(std::sync::Arc::new(process_manager::ProcessManager::new()));
            app.manage(std::sync::Arc::new(awake_clock::AwakeClock::default()));
            app.manage(provider_service::ProviderService::new()?);
            app.manage(export_service::ExportService::new(
                root.clone(), app.state::<media_loopback::MediaLoopback>().origin().to_owned(),
                app.state::<std::sync::Arc<process_manager::ProcessManager>>().inner().clone(),
                app.state::<std::sync::Arc<awake_clock::AwakeClock>>().inner().clone(),
            ));
            app.manage(bridge::Bridge::new(root.clone()));
            let main_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or("main window configuration missing")?;
            let opener_app = app.handle().clone();
            tauri::WebviewWindowBuilder::from_config(app, main_config)?
                .on_navigation(|url| navigation_allowed(url, cfg!(dev)))
                .on_new_window(move |url, _features| open_new_window(&opener_app, url))
                .build()?;
            startup_stages.push(native_startup_stage(native_started,"native.startup_window",window_started));
            // Keep the startup document responsive while the sole native
            // writer performs existing migrations off the WebView event thread.
            let handle = app.handle().clone();
            let account_root=root.clone();
            tauri::async_runtime::spawn(async move {
                let storage_started = std::time::Instant::now();
                let storage_handle = handle.clone();
                let storage = tauri::async_runtime::spawn_blocking(move || {
                    native_storage::NativeStorage::start(root, &storage_handle)
                })
                .await;
                startup_stages.push(native_startup_stage(native_started,"native.storage",storage_started));
                let result = match storage {
                    Ok(Ok(storage)) => {
                        let backend=match storage.backend(){Ok(backend)=>backend,Err(error)=>{eprintln!("Native runtime initialization failed: {error}");handle.exit(1);return;}};
                        let account_started = std::time::Instant::now();
                        let account=account_service::NativeAccount::new(account_root.clone(),storage.cipher(),backend.clone()).map_err(|error|error.to_string());
                        startup_stages.push(native_startup_stage(native_started,"native.account",account_started));
                        let account=match account {Ok(account)=>account,Err(error)=>{eprintln!("Native account initialization failed: {error}");handle.exit(1);return;}};
                        let account_service=account.service();
                        handle.manage(storage);
                        account.start_notifications(handle.clone());
                        handle.manage(account);
                        let event_handle=handle.clone();
                        let tasks_started = std::time::Instant::now();
                        let ready=task_service::TaskService::new(task_service::TaskServiceOptions{
                            backend:backend.clone(),account:account_service.clone(),data_root:account_root.clone(),
                            machine_token_directory:PathBuf::from(config["paths"]["home"].as_str().unwrap()).join(".aistudio"),
                            home_directory:PathBuf::from(config["paths"]["home"].as_str().unwrap()),
                            temporary_directory:PathBuf::from(config["paths"]["temp"].as_str().unwrap()),
                            media_origin:handle.state::<media_loopback::MediaLoopback>().origin().into(),
                            tools:task_tools::resolve(std::path::Path::new(config["resourcesPath"].as_str().unwrap()),std::path::Path::new(config["appPath"].as_str().unwrap()),config["isPackaged"]==true),
                            manager:handle.state::<std::sync::Arc<process_manager::ProcessManager>>().inner().clone(),
                            process_clock:handle.state::<std::sync::Arc<awake_clock::AwakeClock>>().inner().clone(),
                            runtime:tokio::runtime::Handle::current(),
                            committed:std::sync::Arc::new(move|commit|{
                                if let Some(service)=event_handle.try_state::<desktop_service::DesktopService>(){service.events(&commit.events);}
                            }),
                            warning:std::sync::Arc::new(|error|eprintln!("Native task operation failed: {}",error.code)),
                        });
                        startup_stages.push(native_startup_stage(native_started,"native.task_services",tasks_started));
                        let ready=match ready{Ok(ready)=>ready,Err(error)=>{eprintln!("Native task services initialization failed: {}",error.code);handle.exit(1);return;}};
                        // Construction registers all executors without starting
                        // another task owner; the app starts this one graph below.
                        handle.manage(ready);
                        let desktop_started = std::time::Instant::now();
                        let desktop = desktop_service::DesktopService::new(handle.clone(),account_root.clone(),backend.clone(),account_service,
                            env!("AISTUDIO_NATIVE_BUILD_ID").into(),PathBuf::from(config["paths"]["userData"].as_str().unwrap()));
                        startup_stages.push(native_startup_stage(native_started,"native.desktop_services",desktop_started));
                        match desktop {
                            Ok(service)=>{
                                handle.manage(service);
                                match desktop_lifecycle::NativeLifecycle::new(handle.clone(),backend,PathBuf::from(config["paths"]["userData"].as_str().unwrap())) {
                                    Ok(lifecycle)=>{
                                        let diagnostics=lifecycle.diagnostics.clone();
                                        startup_stages.push(native_startup_stage(native_started,"native.initialize",native_started));
                                        for observation in startup_stages {
                                            diagnostics.record("startup.stage",observation);
                                        }
                                        handle.manage(lifecycle);
                                        handle.manage(agent_service::AgentService::new(handle.clone(),account_root,
                                            PathBuf::from(config["paths"]["temp"].as_str().unwrap()),
                                            std::sync::Arc::new(move|event,value|diagnostics.record(event,value))));
                                        handle.state::<desktop_lifecycle::NativeLifecycle>().start().await
                                    },Err(error)=>Err(error),
                                }
                            },
                            Err(error)=>Err(error),
                        }
                    }
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(_) => Err("原生数据库初始化线程退出".to_owned()),
                };
                if let Err(message) = result {
                    let _ = native::dispatch(
                        handle.clone(),
                        "dialog.error",
                        json!({"options":{
                            "title":"小熊AI 原生服务启动失败", "message":message
                        }}),
                    )
                    .await;
                    handle.exit(1);
                }
            });
            match power_events::PowerEvents::start(app.handle()) {
                Ok(observer) => {
                    app.manage(observer);
                }
                Err(error) => eprintln!("系统睡眠/唤醒监测不可用：{error}"),
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("小熊AI 桌面初始化失败：请检查运行时、资源和系统 WebView");
    app.run(|app, event| match event {
        tauri::RunEvent::ExitRequested { api, code, .. } => {
            let state = app.state::<bridge::Bridge>();
            if code.is_none() && !state.exited.load(std::sync::atomic::Ordering::SeqCst) {
                api.prevent_exit();
                desktop_lifecycle::request_exit(app,"quit");
            }
        }
        tauri::RunEvent::Exit => {
            let diagnostics=app.try_state::<desktop_lifecycle::NativeLifecycle>().map(|owner|owner.diagnostics.clone());
            macro_rules! cleanup {
                ($stage:literal,$run:expr) => {{
                    if let Some(log)=diagnostics.as_ref(){log.shutdown_step($stage,||$run)}else{$run}
                }};
            }
            // Final startup/OS exits still release independent resources. Keep
            // the diagnostic writer alive until those actual owners have returned.
            if let Some(lifecycle)=app.try_state::<desktop_lifecycle::NativeLifecycle>(){
                let result=tauri::async_runtime::block_on(async{
                    tokio::time::timeout(std::time::Duration::from_millis(2500),lifecycle.drain()).await
                });
                if !matches!(result,Ok(Ok(()))){lifecycle.diagnostics.record("shutdown.stage",json!({"stage":"shutdown.tasks","action":"interrupted","code":"NATIVE_FINAL_DRAIN_INCOMPLETE"}));}
            }
            if let Err(error)=cleanup!("shutdown.exports",app.state::<export_service::ExportService>().shutdown()){
                if let Some(log)=diagnostics.as_ref(){log.record("shutdown.stage",json!({"stage":"shutdown.exports","action":"failed","code":error.code}));}
            }
            let _=cleanup!("shutdown.providers",tauri::async_runtime::block_on(app.state::<provider_service::ProviderService>().shutdown()));
            let _:Result<(),()>=cleanup!("shutdown.processes",{app.state::<std::sync::Arc<process_manager::ProcessManager>>().shutdown();Ok(())});
            app.state::<bridge::Bridge>().kill();
            if let Some(storage)=app.try_state::<native_storage::NativeStorage>(){let _=cleanup!("shutdown.storage",storage.shutdown());}
            if let Some(media)=app.try_state::<media_loopback::MediaLoopback>(){let _:Result<(),()>=cleanup!("shutdown.media",{media.stop();Ok(())});}
            if let Some(observer)=app.try_state::<power_events::PowerEvents>(){let _:Result<(),()>=cleanup!("shutdown.observer",{observer.stop();Ok(())});}
            let _:Result<(),()>=cleanup!("shutdown.assertions",{native::release_power_assertions();Ok(())});
            if let Some(log)=diagnostics {
                log.record("app_quit",json!({}));
                let _=log.close();
            }
        }
        _ => {}
    });
}

#[tauri::command]
fn media_origin(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    if !bridge::trusted_window(&window) {
        return Err("DESKTOP_ORIGIN_DENIED".into());
    }
    Ok(Some(
        app.state::<media_loopback::MediaLoopback>()
            .origin()
            .to_string(),
    ))
}

#[cfg(test)]
mod navigation_tests {
    use super::navigation_allowed;

    #[test]
    fn application_documents_and_fixed_development_origin_are_preserved() {
        for url in [
            "tauri://localhost/startup.html",
            "http://tauri.localhost/index.html#/settings",
        ] {
            assert!(navigation_allowed(&url.parse().unwrap(), false), "{url}");
        }
        let development = "http://127.0.0.1:17300/index.html".parse().unwrap();
        assert!(navigation_allowed(&development, true));
        assert!(!navigation_allowed(&development, false));
    }

    #[test]
    fn only_owned_loopback_plugin_ui_routes_are_available_to_iframes() {
        for url in [
            "http://127.0.0.1:48123/plugins/alpha/ui/index.html",
            "http://127.0.0.1:48123/plugins/A-b_1/ui/pages/settings.html?mode=edit#form",
        ] {
            assert!(navigation_allowed(&url.parse().unwrap(), false), "{url}");
        }
        for url in [
            "http://127.0.0.1:48123/",
            "http://127.0.0.1:48123/plugin-sdk.js",
            "http://127.0.0.1:48123/plugins/alpha/main.py",
            "http://127.0.0.1:48123/plugins/alpha/ui/",
            "http://127.0.0.1:48123/plugins/_alpha/ui/index.html",
            "http://127.0.0.1:48123/plugins/alpha/ui/%2e%2e%2fmain.py",
            "http://127.0.0.1:48123/plugins/alpha/ui/private%5cfile.html",
            "http://127.0.0.1:48123/plugins/alpha/ui/private%00file.html",
            "http://127.0.0.1/plugins/alpha/ui/index.html",
            "http://127.0.0.1:0/plugins/alpha/ui/index.html",
            "http://localhost:48123/plugins/alpha/ui/index.html",
            "http://127.0.0.2:48123/plugins/alpha/ui/index.html",
        ] {
            assert!(!navigation_allowed(&url.parse().unwrap(), false), "{url}");
        }
    }

    #[test]
    fn remote_origins_credentials_and_non_web_schemes_cannot_navigate_main() {
        for url in [
            "https://example.com/",
            "http://tauri.localhost.evil.example/",
            "tauri://evil.example/index.html",
            "tauri://localhost:123/index.html",
            "http://user:secret@tauri.localhost/index.html",
            "http://user:secret@127.0.0.1:17300/index.html",
            "file:///private.txt",
            "javascript:alert(1)",
            "data:text/html,untrusted",
            "about:blank",
        ] {
            assert!(!navigation_allowed(&url.parse().unwrap(), true), "{url}");
        }
    }
}


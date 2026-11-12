//! Native public desktop facade. OS effects borrow the application's existing
//! owners; no Node phase callback, new writer or second upload owner exists.
use aistudio_desktop_api::{DesktopApi,DesktopEffects,EffectFuture,NativeUploads,Reply,SaveDialog,TransferHost,SearchHost};
use aistudio_runtime_backend::RuntimeBackend;
use serde_json::{json,Value};
use std::{path::{Path,PathBuf},sync::{Arc,Mutex}};
use tauri::{Emitter,Manager};

fn native_error(message:String)->aistudio_desktop_api::Error {
    let(code,detail)=message.split_once(": ").unwrap_or(("NATIVE_OPERATION_FAILED",&message));
    aistudio_desktop_api::error(code,detail)
}
pub struct NativeEffects{app:tauri::AppHandle}
impl NativeEffects {
    async fn save(&self,dialog:SaveDialog)->aistudio_desktop_api::Result<Option<PathBuf>> {
        let filename=PathBuf::from(dialog.default_name);
        // A preferred starting folder is optional; an unset XDG Downloads directory
        // must not prevent the user from choosing a real save destination.
        let default=if filename.is_absolute(){filename}else{self.app.path().download_dir().map(|directory|directory.join(&filename)).unwrap_or(filename)};
        let options=json!({"title":dialog.title,"defaultPath":default,
            "filters":[{"name":dialog.extension,"extensions":[dialog.extension]}]});
        let reply=crate::native::dispatch(self.app.clone(),"dialog.save",json!({"options":options})).await.map_err(native_error)?;
        Ok(reply["filePath"].as_str().map(PathBuf::from))
    }
    async fn external(&self,url:&str)->aistudio_desktop_api::Result<()> {
        crate::native::dispatch(self.app.clone(),"shell.open-external",json!({"url":url})).await.map_err(native_error)?;
        Ok(())
    }
    fn browser(&self,url:&str)->aistudio_desktop_api::Result<()> {
        let url=crate::native::http_url(url).map_err(native_error)?;
        let app=self.app.clone();
        let label=format!("external-{}",rand::random::<u64>());
        let mut builder=tauri::WebviewWindowBuilder::new(&app,label,tauri::WebviewUrl::External(url))
            .title("小熊AI").inner_size(1600.0,900.0)
            .on_navigation(|url|matches!(url.scheme(),"http"|"https")&&url.username().is_empty()&&url.password().is_none());
        if let Some(parent)=app.get_webview_window("main"){
            builder=builder.parent(&parent).map_err(|error|native_error(error.to_string()))?;
        }
        let window=builder.build().map_err(|error|native_error(error.to_string()))?;
        window.maximize().map_err(|error|native_error(error.to_string()))?;
        Ok(())
    }
}
impl TransferHost for NativeEffects {
    fn choose_settings_file(&self)->EffectFuture<'_,Option<PathBuf>>{Box::pin(async move{
        let reply=crate::native::dispatch(self.app.clone(),"dialog.open",json!({"options":{
            "title":"导入公共配置","properties":["openFile"],"filters":[{"name":"JSON","extensions":["json"]}]
        }})).await.map_err(native_error)?;
        if reply["canceled"]==true{return Ok(None);}
        reply["filePaths"].as_array().and_then(|paths|paths.first()).and_then(Value::as_str).map(PathBuf::from)
            .map(Some).ok_or_else(||native_error("NATIVE_DIALOG_RESULT_INVALID: 原生打开位置结果无效".into()))
    })}
}
impl SearchHost for NativeEffects {
    fn query(&self,args:Value)->aistudio_desktop_api::Result<Value>{
        self.app.state::<crate::native_storage::NativeStorage>().search_query(args).map_err(native_error)
    }
}
impl DesktopEffects for NativeEffects {
    fn choose_save<'a>(&'a self,dialog:SaveDialog)->EffectFuture<'a,Option<PathBuf>>{Box::pin(self.save(dialog))}
    fn reveal<'a>(&'a self,path:&'a Path)->EffectFuture<'a,()>{Box::pin(async move{
        crate::native::dispatch(self.app.clone(),"shell.show-item",json!({"path":path})).await.map_err(native_error)?;Ok(())
    })}
    fn open_external<'a>(&'a self,url:&'a str,fallback:bool)->EffectFuture<'a,Value>{Box::pin(async move{
        match self.external(url).await {
            Ok(())=>Ok(json!({"success":true,"opened":"external"})),
            Err(error) if fallback=>{self.browser(url)?;Ok(json!({"success":true,"opened":"internal-fallback","error":error.message}))},
            Err(error)=>Err(error),
        }
    })}
    fn open_tutorial<'a>(&'a self,url:&'a str)->EffectFuture<'a,()>{Box::pin(self.external(url))}
}
struct Uploads{app:tauri::AppHandle,backend:Arc<RuntimeBackend>}
impl NativeUploads for Uploads {
    fn commit(&self,id:&str,options:Value)->aistudio_desktop_api::Result<Reply>{
        let mut commit_error=None;
        let media=self.app.state::<crate::media_loopback::MediaLoopback>();
        let result=media.commit_verified_upload(id,|receipt|{
            match self.backend.ingest_verified_upload(receipt,options){
                Ok(commit)=>Ok(Reply{value:commit.value,events:commit.events}),
                Err(error)=>{commit_error=Some(error.into());Err("Native media commit failed".into())}
            }
        });
        match result{Ok(reply)=>Ok(reply),Err(message)=>Err(commit_error.unwrap_or_else(||native_error(message)))}
    }
}
pub struct DesktopService {
    pub api:Arc<DesktopApi>,
    pub builds:Mutex<crate::build_identity::BuildIdentity>,
    app:tauri::AppHandle,
    pub account:Arc<aistudio_account::AccountService>,
    pub chatgpt_relay:Arc<aistudio_chatgpt_relay::RelayService>,
    public_media:Arc<crate::desktop_public_media::DesktopPublicMedia>,
    pub package:crate::desktop_package_sharing::NativePackage,
    pub updates:Arc<aistudio_updates::Controller>,
}
impl DesktopService {
    pub fn new(app:tauri::AppHandle,root:PathBuf,backend:Arc<RuntimeBackend>,account:Arc<aistudio_account::AccountService>,main_build:String,profile_directory:PathBuf)->Result<Self,String> {
        let effects=Arc::new(NativeEffects{app:app.clone()});
        let uploads=Arc::new(Uploads{app:app.clone(),backend:backend.clone()});
        let exports=Arc::new(crate::desktop_exports::DesktopExports::new(app.clone(),root.clone(),backend.clone()));
        let os=Arc::new(crate::desktop_os::DesktopOs::new(app.clone()));
        let tasks=app.state::<crate::task_service::TaskService>();
        let services=tasks.application.host.services().map_err(|error|error.to_string())?;
        let plugins=Arc::new(crate::desktop_plugins::DesktopPlugins{app:app.clone(),owner:app.state::<crate::task_service::TaskService>().plugins.clone(),channels:tasks.channel_plugins.clone(),network:tasks.network.clone(),clock:services.context.clock.clone()});
        let commit_app=app.clone(); let status_app=app.clone();
        let chatgpt_relay=crate::task_chatgpt::create(crate::task_chatgpt::ChatgptHostOptions{
            backend:backend.clone(),manual:tasks.manual_relay.clone(),browsers:tasks.browsers.clone(),
            clock:services.context.clock.clone(),network:tasks.network.clone(),profile_directory:profile_directory.clone(),
            home_directory:app.path().home_dir().map_err(|error|error.to_string())?,temporary_directory:std::env::temp_dir(),
            committed:Arc::new(move|commit|{if let Some(service)=commit_app.try_state::<DesktopService>(){service.events(&commit.events);}}),
            status_changed:Arc::new(move|status|{let _=status_app.emit_to("main","desktop://event",json!({"channel":"chatgpt-relay:status","args":[status]}));}),
        }).map_err(|error|error.to_string())?;
        let published_app=app.clone();
        let public_media=Arc::new(crate::desktop_public_media::DesktopPublicMedia{
            publisher:tasks.publisher.clone(),channels:tasks.official_storage.clone(),catalog:Arc::new(aistudio_task_host::ProviderCatalog::new(backend.clone())),client:tasks.provider_client.clone(),media:tasks.media.clone(),
            clock:services.context.clock.clone(),cancellation:Default::default(),
            published:Arc::new(move|sha|{if let Some(service)=published_app.try_state::<DesktopService>(){service.events(&[json!({"entity":"public-media","sha":sha})]);}}),
        });
        let package=crate::desktop_package_sharing::NativePackage::new(app.clone(),root.clone(),backend.clone(),account.clone(),&tasks,os.clone()).map_err(|error|error.to_string())?;
        let(updates,install)=crate::desktop_updates::create(app.clone(),backend.clone(),profile_directory).map_err(|e|e.to_string())?;
        let update_download=crate::task_updates::UpdateDownload::register(tasks.application.clone(),backend.clone(),updates.clone()).map_err(|e|e.to_string())?;
        let api=DesktopApi::new(root,backend,account.clone(),effects.clone()).with_uploads(uploads)
            .with_updates(updates.clone(),install).with_update_download_host(update_download).with_task_recovery(tasks.recovery_host.clone())
            .with_package_sharing(package.service.clone()).with_task_preparation(tasks.preparation.clone())
            .with_application(tasks.application.clone()).with_manual_relay(tasks.manual_relay.clone())
            .with_chatgpt_relay(chatgpt_relay.clone()).with_public_media(public_media.clone())
            .with_local_host(Arc::new(crate::desktop_local::DesktopLocal{app:app.clone()}))
            .with_transfer_host(effects.clone()).with_search_host(effects)
            .with_media_export_host(exports.clone()).with_media_probe_host(exports)
            .with_directory_host(os.clone()).with_network_proxy_host(os.clone()).with_project_host(os)
            .with_plugin_host(plugins)
            .with_joojo_login_host(Arc::new(crate::joojo_login::JoojoLogin::new(app.clone())));
        Ok(Self{api:Arc::new(api),builds:Mutex::new(crate::build_identity::BuildIdentity::new(main_build)),app,account,chatgpt_relay,public_media,package,updates})
    }
    pub async fn shutdown(&self)->Result<(),String>{
        self.updates.shutdown_for_exit().await;
        let package=self.package.shutdown().await;
        self.public_media.shutdown();
        let relay=self.chatgpt_relay.shutdown().await;
        self.api.shutdown_media_exports().await;
        package.map_err(|error|error.to_string())?;
        relay.map_err(|error|error.to_string())?;
        Ok(())
    }
    /// Transfer each complete public execution chain explicitly. Task controls
    /// keep their current dispatcher until its full replacement is ready.
    pub fn routes(&self,channel:&str)->bool {
        DesktopApi::supports(channel)
    }
    pub async fn invoke(&self,channel:String,args:Vec<Value>,undefined_paths:Vec<String>)->Result<Value,Value>{
        let api=self.api.clone();let runtime=tokio::runtime::Handle::current();
        let reply=tauri::async_runtime::spawn_blocking(move||runtime.block_on(api.invoke(&channel,&args,&undefined_paths)))
            .await.map_err(|_|json!({"name":"Error","code":"NATIVE_DESKTOP_WORKER_FAILED","message":"原生桌面操作线程退出"}))?;
        self.events(&reply.events);
        Ok(reply.value)
    }
    pub fn host_build(&self,value:&Value){
        self.builds.lock().unwrap().set_host(value);
        self.attention();
    }
    pub fn verified(&self,window:&str,value:&Value)->Value {
        let result=self.builds.lock().unwrap().verify(window,value);
        self.attention();result
    }
    pub fn guard_write(&self,window:&str,channel:&str)->Result<(),Value> {
        if crate::build_identity::read_only(channel){return Ok(());}
        let final_save=channel=="data:apply" && self.app.state::<crate::renderer_flush::RendererFlush>().awaiting(window);
        if !final_save && self.app.try_state::<crate::desktop_lifecycle::NativeLifecycle>().is_some_and(|lifecycle|lifecycle.write_fence.load(std::sync::atomic::Ordering::Acquire)){
            return Err(json!({"code":"UPDATE_HOST_BUSY","message":"应用正在保存并退出，暂不接受新的编辑操作"}));
        }
        self.account.assert_local().map_err(|error|json!({"name":"Error","code":error.code,"message":error.message,"details":{}}))?;
        let result=self.builds.lock().unwrap().assert_writable(window);
        self.attention();result
    }
    fn attention(&self){
        let notices=self.builds.lock().unwrap().take_attention();
        for code in notices{let _=self.app.emit_to("main","desktop://event",json!({"channel":"host:build-mismatch","args":[{"code":code}]}));}
    }
    /// Native events are sent only after the transaction has committed. A
    /// failed event consumer cannot turn a committed write into a retry.
    pub fn events(&self,events:&[Value]) {
        if let Some(tasks)=self.app.try_state::<crate::task_service::TaskService>(){tasks.mark_events(events);}
        if events.iter().any(|event|!aistudio_task_host::is_task_progress_event(event) && matches!(event["entity"].as_str(),Some("task"|"taskTerminal"|"settings"))) {
            self.chatgpt_relay.notify();
        }
        if !events.is_empty(){self.app.state::<crate::native_storage::NativeStorage>().notify_committed(events);}
        for event in events {
            let _=self.app.emit_to("main","desktop://event",json!({"channel":"local-data:changed","args":[event]}));
        }

    }
}

use crate::{cloud_route, disk, error, now, public_profile, service_url, Result, SecretCipher};
use aistudio_provider_io::{CancellationToken, ProviderClient, ProviderError};
use reqwest::{
    header::{HeaderMap, HeaderValue, SET_COOKIE},
    Method, Url,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{broadcast, Mutex as AsyncMutex, Notify};
use zeroize::Zeroizing;

pub struct AccountOptions {
    pub directory: PathBuf,
    pub media_directory: PathBuf,
    pub default_service_url: String,
    pub timeout: Duration,
    pub media_idle_timeout: Duration,
}
#[derive(Clone)]
struct State {
    service_url: String,
    cookies: BTreeMap<String, String>,
    profile: Value,
    network: &'static str,
    error: Option<String>,
    epoch: u64,
    verified_at: i64,
    refresh_serial: u64,
    snapshot_revision: u64,
}
struct Flight {
    cancel: CancellationToken,
    done: Arc<Notify>,
    finished: AtomicBool,
}
struct RequestGuard<'a> {
    service: &'a AccountService,
    id: Option<String>,
    flight: Arc<Flight>,
}
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        if let Some(id) = &self.id {
            self.service.requests.lock().unwrap().remove(id);
        }
        self.flight.finished.store(true, Ordering::Release);
        self.flight.done.notify_waiters();
    }
}
pub struct AccountService {
    options: AccountOptions,
    cipher: Arc<dyn SecretCipher>,
    client: ProviderClient,
    state: Mutex<State>,
    action_gate: AsyncMutex<()>,
    refresh_gate: AsyncMutex<()>,
    requests: Mutex<HashMap<String, Arc<Flight>>>,
    events: broadcast::Sender<Value>,
}
impl AccountService {
    pub fn new(
        options: AccountOptions,
        cipher: Arc<dyn SecretCipher>,
        client: ProviderClient,
    ) -> Result<Self> {
        let origin = service_url(&options.default_service_url)?;
        Ok(Self {
            options,
            cipher,
            client,
            state: Mutex::new(State {
                service_url: origin,
                cookies: BTreeMap::new(),
                profile: Value::Null,
                network: "unknown",
                error: None,
                epoch: 0,
                verified_at: 0,
                refresh_serial: 0,
                snapshot_revision: 0,
            }),
            action_gate: AsyncMutex::new(()),
            refresh_gate: AsyncMutex::new(()),
            requests: Mutex::new(HashMap::new()),
            events: broadcast::channel(128).0,
        })
    }
    /// Opaque in-process binding for external application grants. Never expose
    /// this identity, cookies or session state through an Agent response.
    pub fn agent_authorization_context(&self) -> String {
        let s = self.state.lock().unwrap();
        json!([s.epoch, s.service_url, s.profile["id"], s.profile["email"]]).to_string()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    pub fn snapshot(&self) -> Value {
        let s = self.state.lock().unwrap();
        self.snapshot_unlocked(&s)
    }
    pub fn snapshot_with_revision(&self) -> (u64, Value) {
        let mut s = self.state.lock().unwrap();
        s.snapshot_revision += 1;
        (s.snapshot_revision, self.snapshot_unlocked(&s))
    }
    fn publish(&self) -> Value {
        let value = self.snapshot();
        let _ = self.events.send(value.clone());
        value
    }
    pub fn assert_local(&self) -> Result<()> {
        if self.snapshot()["localAllowed"] == true {
            Ok(())
        } else {
            Err(error("ACCOUNT_REQUIRED", "请先登录、验证邮箱并激活账户"))
        }
    }
    fn encryption_ready(&self) -> Result<()> {
        if self.cipher.available() {
            Ok(())
        } else {
            Err(error(
                "ACCOUNT_STORAGE_UNAVAILABLE",
                "系统安全存储不可用，无法保存账户会话",
            ))
        }
    }
    fn persist(&self, s: &State) -> Result<()> {
        std::fs::create_dir_all(&self.options.directory).map_err(disk)?;
        let local=self.cipher.local_storage();
        let path = self.options.directory.join(if local {"account-session.local.json"} else {"account-session.enc"});
        if !local && s.cookies.is_empty() && s.profile.is_null() {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(disk(e)),
            };
            return Ok(());
        }
        self.encryption_ready()?;
        let clear = Zeroizing::new(
            json!({"version":1,"serviceUrl":s.service_url,"cookies":s.cookies,"profile":s.profile})
                .to_string(),
        );
        if local {
            let mut saved:Value=serde_json::from_str(&clear).map_err(|_|error("ACCOUNT_STORAGE_INVALID","账户本地配置无效"))?;
            saved["format"]=json!("aistudio-local-account");
            return atomic_write(&path,saved.to_string().as_bytes());
        }
        let encrypted = self.cipher.encrypt(&clear).map_err(|_| {
            error(
                "ACCOUNT_STORAGE_UNAVAILABLE",
                "系统安全存储不可用，无法保存账户会话",
            )
        })?;
        atomic_write(&path, &encrypted)
    }
    pub fn load(&self) -> Value {
        let result = (|| -> Result<()> {
            let mut s = self.state.lock().unwrap();
            match std::fs::read(self.options.directory.join("account-server.json")) {
                Ok(bytes) => {
                    let v: Value = serde_json::from_slice(&bytes)
                        .map_err(|_| error("ACCOUNT_STORAGE_INVALID", "账户服务配置无效"))?;
                    s.service_url = service_url(v["serviceUrl"].as_str().unwrap_or(""))?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(disk(e)),
            }
            let local=self.cipher.local_storage();
            match std::fs::read(self.options.directory.join(if local {"account-session.local.json"} else {"account-session.enc"})) {
                Ok(bytes) => {
                    let clear = if local {
                        Zeroizing::new(String::from_utf8(bytes).map_err(|_|error("ACCOUNT_STORAGE_INVALID","本地账户配置无效"))?)
                    } else {
                        self.encryption_ready()?;
                        Zeroizing::new(self.cipher.decrypt(&bytes).map_err(|_|error("ACCOUNT_STORAGE_INVALID","读取账户安全存储失败，请主动导入旧会话或重新登录"))?)
                    };
                    let saved: Value = serde_json::from_str(&clear).map_err(|_| {
                        error("ACCOUNT_STORAGE_INVALID", "账户会话文件不匹配，请重新登录")
                    })?;
                    // An empty local logout marker carries no service identity.
                    // Keep it across server changes so legacy sessions stay revoked.
                    let revoked = local
                        && saved["cookies"].as_object().is_some_and(|cookies| cookies.is_empty())
                        && saved.get("profile") == Some(&Value::Null);
                    if (local && saved["format"] != "aistudio-local-account") || saved["version"] != 1
                        || (!revoked && saved["serviceUrl"] != s.service_url)
                        || !saved["cookies"].is_object()
                    {
                        return Err(error(
                            "ACCOUNT_STORAGE_INVALID",
                            "账户会话文件不匹配，请重新登录",
                        ));
                    }
                    s.cookies = saved["cookies"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .filter_map(|(k, v)| {
                            v.as_str()
                                .filter(|v| cookie_valid(k, v))
                                .map(|v| (k.clone(), v.into()))
                        })
                        .collect();
                    s.profile = if saved["profile"].is_null() {
                        Value::Null
                    } else {
                        public_profile(&saved["profile"])?
                    };
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(disk(e)),
            }
            s.verified_at = 0;
            Ok(())
        })();
        if let Err(e) = result {
            let mut s = self.state.lock().unwrap();
            s.cookies.clear();
            s.profile = Value::Null;
            s.error = Some(e.message);
        }
        // Loading a cached session changes the same live authority as an
        // action. Native renderer/task subscribers must see that transition.
        self.publish()
    }
    fn import_legacy_session(&self) -> Result<Value> {
        if !self.cipher.local_storage() {return Err(error("ACCOUNT_ACTION_INVALID","当前模式不支持本地会话导入"));}
        let path=self.options.directory.join("account-session.local.json");
        if path.try_exists().map_err(disk)? {return Ok(json!({"imported":false,"existingLocalSession":true}));}
        let bytes=match std::fs::read(self.options.directory.join("account-session.enc")) {
            Ok(value)=>value,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(json!({"imported":false,"legacyMissing":true})),Err(e)=>return Err(disk(e))
        };
        let original_epoch=self.state.lock().unwrap().epoch;
        let clear=Zeroizing::new(self.cipher.decrypt(&bytes).map_err(|_|error("ACCOUNT_STORAGE_INVALID","旧账户会话导入失败，原数据保留，可解锁系统存储后重试或重新登录"))?);
        let saved:Value=serde_json::from_str(&clear).map_err(|_|error("ACCOUNT_STORAGE_INVALID","旧账户会话格式无效"))?;
        let mut state=self.state.lock().unwrap();
        if state.epoch!=original_epoch || path.try_exists().map_err(disk)? {return Err(error("ACCOUNT_SESSION_CHANGED","账户状态已更新，未覆盖当前本地会话"));}
        if saved["version"]!=1 || saved["serviceUrl"]!=state.service_url || !saved["cookies"].is_object() {return Err(error("ACCOUNT_STORAGE_INVALID","旧账户会话与当前服务不匹配"));}
        let mut next=state.clone();
        next.cookies=saved["cookies"].as_object().unwrap().iter().filter_map(|(k,v)|v.as_str().filter(|v|cookie_valid(k,v)).map(|v|(k.clone(),v.into()))).collect();
        next.profile=if saved["profile"].is_null(){Value::Null}else{public_profile(&saved["profile"])?};
        next.epoch+=1;next.verified_at=0;next.error=None;
        self.persist(&next)?;
        *state=next;drop(state);self.publish();
        Ok(json!({"imported":true,"originalRetained":true}))
    }
    fn clear(&self, retain_profile: bool) -> Result<Value> {
        let mut s = self.state.lock().unwrap();
        s.epoch += 1;
        s.cookies.clear();
        if !retain_profile {
            s.profile = Value::Null;
        }
        s.verified_at = 0;
        s.network = "unknown";
        s.error = None;
        self.persist(&s)?;
        drop(s);
        Ok(self.publish())
    }
    async fn request(
        &self,
        route: &str,
        method: Method,
        body: Option<Value>,
        authenticated: bool,
        replace: Option<bool>,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        self.request_options(route, method, body, authenticated, replace, cancel, None)
            .await
    }
    async fn request_options(
        &self,
        route: &str,
        method: Method,
        body: Option<Value>,
        authenticated: bool,
        replace: Option<bool>,
        cancel: &CancellationToken,
        options: Option<&Value>,
    ) -> Result<Value> {
        let (origin, epoch, cookies) = {
            let s = self.state.lock().unwrap();
            (
                s.service_url.clone(),
                s.epoch,
                if authenticated {
                    s.cookies
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join("; ")
                } else {
                    String::new()
                },
            )
        };
        let target = Url::parse(&origin)
            .and_then(|u| u.join(route))
            .map_err(|_| error("ACCOUNT_ROUTE_INVALID", "账户请求路径无效"))?;
        if !route.starts_with("/api/")
            || route.starts_with("//")
            || route.contains('\\')
            || target.origin().ascii_serialization() != origin
        {
            return Err(error("ACCOUNT_ROUTE_INVALID", "账户请求不能跨服务地址"));
        }
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("origin", HeaderValue::from_str(&origin).unwrap());
        if !cookies.is_empty() {
            let mut v = HeaderValue::from_str(&cookies)
                .map_err(|_| error("ACCOUNT_STORAGE_INVALID", "账户会话格式无效"))?;
            v.set_sensitive(true);
            headers.insert("cookie", v);
        }
        let extra = options.cloned().unwrap_or_else(|| {
            body.map(|body| json!({"body":body}))
                .unwrap_or_else(|| json!({}))
        });
        let media = extra.get("uploadMedia").is_some() || extra.get("downloadMedia").is_some();
        let pending = crate::media::transfer(
            &self.client,
            &self.options.media_directory,
            method,
            target,
            headers,
            &extra,
            cancel,
            if media {
                self.options.media_idle_timeout
            } else {
                self.options.timeout
            },
            || self.state.lock().unwrap().epoch == epoch,
        );
        let result = if media {
            pending.await
        } else {
            match tokio::time::timeout(self.options.timeout, pending).await {
                Ok(result) => result,
                Err(_) => Err(error(
                    "ACCOUNT_TIMEOUT",
                    "账户服务请求超时，请稍后重试；已激活的本地功能可继续使用",
                )),
            }
        };
        if self.state.lock().unwrap().epoch != epoch {
            return Err(error("ACCOUNT_SESSION_CHANGED", "账户已切换，请重新操作"));
        }
        let crate::media::Response {
            reply: response,
            value,
            mut incoming,
        } = match result {
            Ok(v) => v,
            Err(e) => {
                if matches!(e.code.as_str(), "ACCOUNT_TIMEOUT" | "ACCOUNT_OFFLINE") {
                    self.state.lock().unwrap().network = "offline";
                }
                return Err(e);
            }
        };
        if (300..400).contains(&response.status) {
            return Err(error(
                "ACCOUNT_REDIRECT_REJECTED",
                "账户服务意外重定向，请检查服务地址",
            ));
        }
        if !(200..300).contains(&response.status) {
            let code = value["code"]
                .as_str()
                .or_else(|| value["error"]["code"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("ACCOUNT_HTTP_{}", response.status));
            let message = value["message"]
                .as_str()
                .or_else(|| value["error"]["message"].as_str())
                .unwrap_or("账户请求失败")
                .chars()
                .take(300)
                .collect::<String>();
            let mut e = if matches!(response.status, 408 | 425 | 429 | 500..=599) {
                error(
                    &format!("ACCOUNT_HTTP_{}", response.status),
                    format!("账户服务暂时不可用（HTTP {}），请稍后重试", response.status),
                )
            } else {
                error(&code, message)
            };
            e.status = Some(response.status);
            e.payload = Some(value);
            if authenticated && response.status == 401 {
                self.clear(true)?;
            } else if authenticated
                && matches!(code.as_str(), "ACCOUNT_DISABLED" | "EMAIL_NOT_VERIFIED")
            {
                self.clear(false)?;
            }
            return Err(e);
        }
        let mut s = self.state.lock().unwrap();
        if s.epoch != epoch {
            return Err(error("ACCOUNT_SESSION_CHANGED", "账户已切换，请重新操作"));
        }
        let mut next = s.clone();
        if let Some(retain) = replace {
            next.cookies.clear();
            if !retain {
                next.profile = Value::Null;
            }
            next.network = "unknown";
        }
        for item in response.headers.get_all(SET_COOKIE) {
            let Ok(item) = item.to_str() else { continue };
            let Some((name, value)) = item.split(';').next().unwrap_or("").split_once('=') else {
                continue;
            };
            if !cookie_valid(name, value) {
                continue;
            }
            if value.is_empty()
                || regex::Regex::new("(?i)max-age=0\\b")
                    .unwrap()
                    .is_match(item)
            {
                next.cookies.remove(name);
            } else {
                next.cookies.insert(name.into(), value.into());
            }
        }
        next.network = if replace.is_some() {
            "unknown"
        } else {
            "online"
        };
        next.error = None;
        self.persist(&next)?;
        *s = next;
        incoming.0.take();
        drop(s);
        if replace.is_some() {
            self.publish();
        }
        Ok(value)
    }
    pub async fn refresh(&self, force: bool, cancel: &CancellationToken) -> Result<Value> {
        let before = self.state.lock().unwrap().refresh_serial;
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(error("TASK_CANCELLED","账户请求已取消")),guard=self.refresh_gate.lock()=>guard};
        {
            let s = self.state.lock().unwrap();
            if s.network == "online"
                && !s.profile.is_null()
                && s.verified_at > 0
                && ((!force && now() - s.verified_at < 60_000) || s.refresh_serial != before)
            {
                return Ok(self.snapshot_unlocked(&s));
            }
        }
        let epoch = self.state.lock().unwrap().epoch;
        let value = self
            .request("/api/system/me", Method::GET, None, true, None, cancel)
            .await?;
        let mut s = self.state.lock().unwrap();
        if s.epoch != epoch {
            return Err(error("ACCOUNT_SESSION_CHANGED", "账户已切换，请重新操作"));
        }
        let mut next = s.clone();
        next.profile = public_profile(&value)?;
        next.verified_at = now();
        next.refresh_serial += 1;
        self.persist(&next)?;
        *s = next;
        drop(s);
        Ok(self.publish())
    }
    fn snapshot_unlocked(&self, s: &State) -> Value {
        // Called only for the refresh freshness check; no nested lock.
        let p = &s.profile;
        let local = p["emailVerified"] == true && p["everActivated"] == true;
        let expired = p["validUntil"]
            .as_str()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .is_some_and(|v| v.timestamp_millis() <= now());
        json!({"legacySessionAvailable":self.cipher.local_storage()&&self.options.directory.join("account-session.enc").exists()&&!self.options.directory.join("account-session.local.json").exists(),"serviceUrl":s.service_url,"profile":p,"sessionPresent":!s.cookies.is_empty(),"localAllowed":local,"cloudAllowed":!s.cookies.is_empty()&&local&&!expired&&s.network=="online"&&p["cloudEligible"]==true,"network":s.network,"expiry":if p["everActivated"]!=true{"inactive"}else if expired{"expired-pending"}else{"active"},"error":s.error})
    }
    pub async fn request_cloud(
        &self,
        route: &str,
        options: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        let origin = self.state.lock().unwrap().service_url.clone();
        let route = cloud_route(route, &options, &origin)?;
        let id = Some(
            options["requestId"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        );
        let flight = Arc::new(Flight {
            cancel: cancel.clone(),
            done: Arc::new(Notify::new()),
            finished: AtomicBool::new(false),
        });
        if let Some(id) = &id {
            if id.len() != 36 || uuid::Uuid::parse_str(id).is_err() {
                return Err(error("ACCOUNT_REQUEST_INVALID", "账户请求编号无效或重复"));
            }
            let mut flights = self.requests.lock().unwrap();
            if flights.contains_key(id) {
                return Err(error("ACCOUNT_REQUEST_INVALID", "账户请求编号无效或重复"));
            }
            flights.insert(id.clone(), flight.clone());
        }
        let _guard = RequestGuard {
            service: self,
            id,
            flight,
        };
        let result = async {
            self.refresh(false, &cancel).await?;
            if self.snapshot()["cloudAllowed"] != true {
                return Err(error(
                    "ACCOUNT_CLOUD_REQUIRED",
                    "当前账户无有效云端权限，请续期或检查网络",
                ));
            }
            self.request_options(
                &route,
                Method::from_bytes(
                    options["method"]
                        .as_str()
                        .unwrap_or("GET")
                        .to_uppercase()
                        .as_bytes(),
                )
                .map_err(|_| error("ACCOUNT_ROUTE_INVALID", "云端请求路径未授权"))?,
                options.get("body").cloned(),
                true,
                None,
                &cancel,
                Some(&options),
            )
            .await
        }
        .await;
        if result.is_err() {
            self.publish();
        }
        result
    }
    pub async fn authorized_cloud(
        &self,
        route: &str,
        options: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        self.request_cloud(route, options, cancel).await
    }
    pub async fn action(&self, action: &str, input: Value) -> Result<Value> {
        let cancel = CancellationToken::new();
        if action == "logout" {
            let (origin, cookies) = {
                let s = self.state.lock().unwrap();
                (
                    s.service_url.clone(),
                    s.cookies
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            };
            self.clear(false)?;
            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            headers.insert("origin", HeaderValue::from_str(&origin).unwrap());
            let mut cookie = HeaderValue::from_str(&cookies)
                .map_err(|_| error("ACCOUNT_STORAGE_INVALID", "账户会话格式无效"))?;
            cookie.set_sensitive(true);
            headers.insert("cookie", cookie);
            let request = self.client.request_bytes(
                Method::POST,
                Url::parse(&format!("{origin}/api/auth/sign-out")).unwrap(),
                headers,
                b"{}".to_vec(),
                &cancel,
            );
            if !matches!(tokio::time::timeout(self.options.timeout,request).await,Ok(Ok(reply)) if (200..300).contains(&reply.status))
            {
                self.state.lock().unwrap().error = Some("本地已注销；服务器会话注销未确认".into());
            }
            return Ok(self.publish());
        }
        let _guard = self
            .action_gate
            .try_lock()
            .map_err(|_| error("ACCOUNT_BUSY", "账户操作正在进行，请稍后重试"))?;
        let result=async {
            let required=|key:&str|input[key].as_str().filter(|s|!s.is_empty()).map(str::to_owned).ok_or_else(||error("ACCOUNT_INPUT_INVALID","请填写所需账户信息"));
            match action {
                "importLegacy"=>{self.import_legacy_session()?;},
                "configure"=>{
                    let next_url=service_url(&required("serviceUrl")?)?;
                    if self.state.lock().unwrap().service_url!=next_url {
                        self.clear(false)?;
                        let mut s=self.state.lock().unwrap();
                        std::fs::create_dir_all(&self.options.directory).map_err(disk)?;
                        atomic_write(&self.options.directory.join("account-server.json"),json!({"serviceUrl":next_url}).to_string().as_bytes())?;
                        // clear already persisted the identity-free logout marker.
                        // The server URL has one durable commit and one live owner.
                        s.service_url=next_url;
                    }
                },
                "refresh"=>{let has=!self.state.lock().unwrap().cookies.is_empty();if has{self.refresh(true,&cancel).await?;}},
                "login"|"register"=>{
                    let email=required("email")?;let password=Zeroizing::new(required("password")?);let name=if action=="register"{Some(required("name")?)}else{None};self.encryption_ready()?;
                    let retain={let mut s=self.state.lock().unwrap();s.epoch+=1;s.profile["email"].as_str().is_some_and(|v|v.eq_ignore_ascii_case(&email))};
                    let mut body=json!({"email":email,"password":password.as_str()});if let Some(name)=name{body["name"]=json!(name);}
                    self.request(if action=="login"{"/api/auth/sign-in/email"}else{"/api/auth/sign-up/email"},Method::POST,Some(body),false,Some(retain),&cancel).await?;
                    let has=!self.state.lock().unwrap().cookies.is_empty();if has{self.refresh(true,&cancel).await?;}
                },
                "resendVerification"|"forgotPassword"=>{let origin=self.state.lock().unwrap().service_url.clone();let email=required("email")?;let (route,body)=if action=="forgotPassword"{("/api/auth/request-password-reset",json!({"email":email,"redirectTo":format!("{origin}/account/reset-password")}))}else{("/api/auth/send-verification-email",json!({"email":email,"callbackURL":format!("{origin}/account/verified")}))};self.request(route,Method::POST,Some(body),true,None,&cancel).await?;},
                "resetPassword"=>{let token=Zeroizing::new(required("token")?);let password=Zeroizing::new(required("password")?);self.request("/api/auth/reset-password",Method::POST,Some(json!({"token":token.as_str(),"newPassword":password.as_str()})),false,None,&cancel).await?;},
                "activate"=>{let code=Zeroizing::new(required("code")?);self.request("/api/system/activate",Method::POST,Some(json!({"code":code.as_str()})),true,None,&cancel).await?;self.refresh(true,&cancel).await?;},
                _=>return Err(error("ACCOUNT_ACTION_INVALID","不支持的账户操作")),
            }
            Ok(self.publish())
        }.await;
        if let Err(e) = &result {
            let same = {
                let s = self.state.lock().unwrap();
                s.profile["email"]
                    .as_str()
                    .zip(input["email"].as_str())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            };
            if matches!(action, "login" | "register")
                && matches!(e.code.as_str(), "ACCOUNT_DISABLED" | "EMAIL_NOT_VERIFIED")
                && same
            {
                self.clear(false)?;
            }
            self.state.lock().unwrap().error = Some(e.message.clone());
            self.publish();
        }
        result
    }
    pub async fn cancel_cloud_request(&self, id: &str) -> Value {
        let flight = self.requests.lock().unwrap().get(id).cloned();
        if let Some(flight) = flight {
            let done = flight.done.notified();
            tokio::pin!(done);
            done.as_mut().enable();
            flight.cancel.cancel();
            if !flight.finished.load(Ordering::Acquire) {
                done.await;
            }
            json!({"cancelled":true})
        } else {
            json!({"cancelled":false})
        }
    }
    pub async fn shutdown(&self) {
        let flights = self
            .requests
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for flight in flights {
            let done = flight.done.notified();
            tokio::pin!(done);
            done.as_mut().enable();
            flight.cancel.cancel();
            if self
                .requests
                .lock()
                .unwrap()
                .values()
                .any(|v| Arc::ptr_eq(v, &flight))
            {
                done.await;
            }
        }
    }
}
fn cookie_valid(name: &str, value: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && !value.contains(['\r', '\n', ';'])
}
pub(crate) fn transport(e: ProviderError) -> crate::Error {
    match e.code.as_str() {
        "TASK_CANCELLED" => error("TASK_CANCELLED", "账户请求已取消"),
        "HTTP_TIMEOUT" => error(
            "ACCOUNT_TIMEOUT",
            "账户服务请求超时，请稍后重试；已激活的本地功能可继续使用",
        ),
        _ => error(
            "ACCOUNT_OFFLINE",
            "无法连接账户服务；已激活的本地功能可继续使用",
        ),
    }
}
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let temp = path.with_file_name(format!(".account-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&temp).map_err(disk)?;
        f.write_all(bytes).map_err(disk)?;
        f.sync_all().map_err(disk)?;
        drop(f);
        replace(&temp, path).map_err(disk)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
#[cfg(not(windows))]
fn replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}
#[cfg(windows)]
fn replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}


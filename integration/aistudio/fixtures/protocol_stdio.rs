//! Test-only cross-language fixture. NEVER link this automatic approver into the app.
//! Uses actual Session/Authorizations/routes, with a controlled business Host.
use aistudio_agent_data::{Authorizations,Error,Host,HostFuture,Result,Session};
use serde_json::{json,Value};
use std::{io::{self,BufRead,Write},sync::Arc,path::PathBuf};
struct Fixture;
impl Host for Fixture {
 fn build_snapshot(&self,_:&str)->Value{json!({"main":"fixture","host":"fixture","writable":true})}
 fn verify_build(&self,_:&str,_:&str)->Result<Value>{Ok(json!({}))}
 fn forget_build(&self,_:&str){}
 fn authorization_context(&self)->Result<String>{Ok("test-only-account".into())}
 fn invoke<'a>(&'a self,channel:&'a str,_:&'a str,args:Vec<Value>)->HostFuture<'a,Value>{Box::pin(async move{match channel{
  "local-data:drafts-get"=>Ok(json!({"id":args[0],"title":"Fixture","episodes":[{"id":"e1","title":"第一集","text":"测试正文"}]})),
  "local-data:drafts-list-summaries"=>Ok(json!([{"id":"draft-1","title":"Fixture"},{"id":"other-draft","title":"Hidden"}])),
  "data:apply"=>Ok(json!({"ok":true,"result":{"commandId":args[0]["commandId"],"replayed":false,"rows":[]}})),
  "account:status"=>Ok(json!({"sessionPresent":true,"localAllowed":true,"cloudAllowed":false,"profile":{"emailVerified":true,"everActivated":true},"cookie":"FIXTURE_SECRET_MUST_NOT_LEAK"})),
  "local-data:channel-config-patch"=>Ok(json!({"apiKey":"FIXTURE_SECRET_MUST_NOT_LEAK"})),
  _=>Err(Error::new("FIXTURE_UNSUPPORTED","Not implemented by controlled test host"))
 }})}
}
fn main(){
 let root=PathBuf::from(std::env::args().nth(1).expect("test root required"));
 let runtime=tokio::runtime::Builder::new_current_thread().build().unwrap();let auth=Arc::new(Authorizations::default());
 let mut session=Session::new(root,"unused-legacy-fixture".into(),Arc::new(Fixture)).with_authorizations(auth.clone());
 for line in io::stdin().lock().lines(){let line=line.unwrap();let request:Value=serde_json::from_str(&line).unwrap();let response=runtime.block_on(session.request(request.clone()));
  if request["method"]=="auth.request"&&response["ok"]==true{auth.decide(response["result"]["requestId"].as_str().unwrap(),true,"test-only-account").unwrap();}
  println!("{}",response);io::stdout().flush().unwrap();
 }
}

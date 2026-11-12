use aistudio_agent_data::*;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
struct OracleHost {
    calls: Mutex<Vec<Value>>,
}
impl Host for OracleHost {
    fn build_snapshot(&self, _: &str) -> Value {
        json!({"main":"test+1"})
    }
    fn verify_build(&self, _: &str, _: &str) -> Result<Value> {
        Ok(json!({"writable":true}))
    }
    fn forget_build(&self, _: &str) {}
    fn invoke<'a>(
        &'a self,
        channel: &'a str,
        _: &'a str,
        args: Vec<Value>,
    ) -> HostFuture<'a, Value> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(json!({"channel":channel,"args":args}));
            Ok(match channel {
                "local-data:drafts-get" => json!({"id":args[0],"title":"中文😀"}),
                "local-data:rows-get" => {
                    json!([{"id":"shot","version":3,"data":{"remark":"before"}}])
                }
                "data:apply" if args[0]["commandId"] == "conflict" => {
                    json!({"ok":false,"error":{"code":"FIELD_CONFLICT","message":"Changed field","details":{"field":"name"}}})
                }
                "data:apply" => {
                    json!({"ok":true,"result":{"commandId":args[0]["commandId"],"replayed":false}})
                }
                "local-data:tasks-list" => {
                    json!([{"id":"live","targetDraftId":"d","type":"image","status":"queued"},{"id":"other","targetDraftId":"e","type":"text","status":"failed"}])
                }
                "task-history:detail" if args[0] == "absent" => Value::Null,
                "task-history:detail" if args[0] == "pruned" => {
                    json!({"row":{"id":"pruned"},"task":null})
                }
                "task-history:detail" => {
                    json!({"row":{"id":args[0]},"task":{"id":args[0],"type":"text","status":"succeeded","checkpoint":{"submit":{"data":{"providerTaskId":"original"}}},"inputSnapshot":{"apiKey":"hidden"},"error":{"code":"SUBMIT_UNKNOWN","taskError":{"code":"SUBMIT_UNKNOWN","class":"unknown_submit"}}}})
                }
                "ai-task:intent" => json!({"tasks":[{"id":args[0]["requestId"]}]}),
                "ai-task:retry" if args[0] == "missing" => {
                    return Err(Error::new("TASK_NOT_FOUND", "No task"))
                }
                _ => {
                    json!({"id":args.first().cloned().unwrap_or_else(||json!("")),"channel":channel})
                }
            })
        })
    }
}
#[tokio::test]
async fn all_business_routes_match_original_node_projection_errors_and_arguments() {
    let valid = json!({"commandId":"edit","draftId":"d","commands":[{"op":"patchShot","shotId":"s","baseVersion":3,"fields":{"remark":"after"}}]});
    let mut cases = vec![
        json!({"method":"draft.get","params":{"draftId":" d "}}),
        json!({"method":"draft.get","params":{"draftId":" "}}),
        json!({"method":"draft.get","params":{"draftId":"\u{feff}"}}),
        json!({"method":"draft.rows","params":{"draftId":"d","entities":[{"entity":"shot","id":"s"}]}}),
        json!({"method":"draft.rows","params":{"draftId":"d","entities":null}}),
        json!({"method":"draft.apply","params":valid}),
        json!({"method":"task.list","params":{}}),
        json!({"method":"task.list","params":{"draftId":"d","status":"queued","type":"image"}}),
        json!({"method":"task.list","params":{"draftId":null}}),
        json!({"method":"task.get","params":{"taskId":"live"}}),
        json!({"method":"task.get","params":{"taskId":"archived"}}),
        json!({"method":"task.get","params":{"taskId":"pruned"}}),
        json!({"method":"task.get","params":{"taskId":"absent"}}),
        json!({"method":"task.get","params":{"taskId":1}}),
        json!({"method":"task.intent","params":{"requestId":"request","options":{"manual":false},"kind":"image"}}),
        json!({"method":"task.intent","params":{"requestId":"request","options":{"manual":true}}}),
        json!({"method":"task.intent","params":{}}),
        json!({"method":"task.cancel","params":{"taskId":"live"}}),
        json!({"method":"task.retry","params":{"taskId":"missing"}}),
        json!({"method":"task.retry","params":{"taskId":"live"}}),
        json!({"method":"task.recheck","params":{"taskId":"live"}}),
        json!({"method":"ai-task:enqueue","params":{}}),
    ];
    for patch in [
        json!({"commandId":"conflict"}),
        json!({"origin":null}),
        json!({"origin":{"kind":"task","taskId":"t"}}),
        json!({"origin":{"kind":"user","windowId":"main"}}),
        json!({"origin":{"kind":"user","windowId":"agent:custom"}}),
        json!({"onMissing":"skip"}),
        json!({"commands":null}),
        json!({"commands":[{"baseVersion":"latest"}]}),
        json!({"commands":[null]}),
    ] {
        let mut params = valid.clone();
        for (key, value) in patch.as_object().unwrap() {
            params[key] = value.clone();
        }
        cases.push(json!({"method":"draft.apply","params":params}));
    }
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/routes-golden.cjs");
    let mut child = Command::new("node")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (index, request) in cases.iter().enumerate() {
        let host = Arc::new(OracleHost {
            calls: Mutex::new(vec![]),
        });
        let mut session = Session::new(
            std::env::current_dir().unwrap(),
            "private".into(),
            host.clone(),
        );
        assert_eq!(session.request(json!({"id":1,"token":"private","method":"hello","params":{"protocol":1,"buildId":"test+1","dataRoot":std::env::current_dir().unwrap()}})).await["ok"], true);
        let mut wire = request.clone();
        wire["id"] = json!(2);
        wire["token"] = json!("private");
        let mut actual = session.request(wire).await;
        actual.as_object_mut().unwrap().remove("id");
        actual["calls"] = json!(host.calls.lock().unwrap().clone());
        assert_eq!(actual, expected[index], "route case {index}: {request}");
    }
}

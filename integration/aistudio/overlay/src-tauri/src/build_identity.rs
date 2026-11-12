//! The existing desktop build identity contract, using a trusted WebView label.
//! A renderer JSON field never selects another window's verified identity.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn read_only(channel: &str) -> bool {
    static READ_ONLY: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    READ_ONLY
        .get_or_init(|| {
            serde_json::from_str(include_str!("../legacy-read-only-channels.json"))
                .expect("desktop read-only contract")
        })
        .contains(channel)
}

pub struct BuildIdentity {
    main: String,
    host: Option<String>,
    renderers: BTreeMap<String, String>,
    agent_schemas: BTreeMap<String, u64>,
    mismatched: bool,
    notified: BTreeSet<String>,
    attention: Vec<String>,
}
impl BuildIdentity {
    pub fn new(main: String) -> Self {
        Self {
            main,
            host: None,
            renderers: BTreeMap::new(),
            agent_schemas: BTreeMap::new(),
            mismatched: false,
            notified: BTreeSet::new(),
            attention: vec![],
        }
    }
    fn notify(&mut self, code: &str) {
        if self.notified.insert(code.into()) {
            self.attention.push(code.into());
        }
    }
    fn compare(&mut self, values: &[String]) {
        if values.iter().all(|value| {
            !value.is_empty()
                && !value.contains("+nobuild")
                && !value.contains("unknown-version+")
                && value == &self.main
        }) {
            return;
        }
        let previous = self.mismatched;
        self.mismatched = true;
        if !previous {
            self.notify("BUILD_MISMATCH");
        }
    }
    pub fn set_host(&mut self, value: &Value) {
        let host = value.as_str().unwrap_or("").trim().to_owned();
        self.host = Some(host.clone());
        self.compare(&[self.main.clone(), host]);
    }
    pub fn verify(&mut self, window: &str, value: &Value) -> Value {
        let renderer = value.as_str().unwrap_or("").trim().to_owned();
        if self
            .renderers
            .get(window)
            .is_some_and(|old| !old.is_empty() && old != &renderer)
        {
            self.mismatched = true;
        }
        self.renderers.insert(window.into(), renderer.clone());
        self.compare(&[self.main.clone(), renderer]);
        if let Some(host) = self.host.clone().filter(|s| !s.is_empty()) {
            self.compare(&[host]);
        }
        let result = self.snapshot(window);
        if result["writable"] == true {
            self.notified.clear();
        }
        result
    }
    pub fn snapshot(&self, window: &str) -> Value {
        json!({"main":self.main,"host":self.host,"renderer":self.renderers.get(window).filter(|s|!s.is_empty()),
            "mismatched":self.mismatched,"writable":!self.mismatched&&self.host.as_ref().is_some_and(|s|!s.is_empty())&&self.renderers.get(window)==Some(&self.main)})
    }
    /// External agents negotiate the stable API schema, not the renderer build.
    /// Native main/host equality and sticky mismatch remain mandatory.
    pub fn negotiate_agent(&mut self, client: &str, schema: u64) -> Result<Value, Value> {
        if !client.starts_with("agent:") || schema != 1 { return Err(json!({"code":"AGENT_SCHEMA_MISMATCH","message":"Unsupported Agent schema"})); }
        self.agent_schemas.insert(client.into(), schema);
        Ok(json!({"schemaVersion":schema,"main":self.main,"host":self.host,"mismatched":self.mismatched}))
    }
    pub fn forget(&mut self, window: &str) {
        self.agent_schemas.remove(window);
        self.renderers.remove(window);
    }
    pub fn assert_writable(&mut self, window: &str) -> Result<(), Value> {
        let agent_writable = self.agent_schemas.get(window) == Some(&1)
            && !self.mismatched && !self.main.is_empty() && !self.main.contains("+nobuild")
            && !self.main.contains("unknown-version+") && self.host.as_ref() == Some(&self.main);
        if self.snapshot(window)["writable"] == true || agent_writable {
            return Ok(());
        }
        let (code, message) = if self.mismatched {
            ("BUILD_MISMATCH","[BUILD_MISMATCH] 页面、主进程或后台任务服务的构建不一致，当前只读；请完全退出并重新启动 小熊AI。")
        } else {
            ("BUILD_UNVERIFIED","[BUILD_UNVERIFIED] 构建版本尚未核对或后台任务服务未就绪，当前只读；请重新启动 小熊AI。")
        };
        self.notify(code);
        Err(json!({"name":"Error","code":code,"message":message,"details":{}}))
    }
    pub fn take_attention(&mut self) -> Vec<String> {
        std::mem::take(&mut self.attention)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 期望值由旧 Node gate（electron/build-compatibility.cjs，经 scripts/native-desktop-check/build-gate-golden.cjs）
    // 在迁移前录成 golden：src-tauri/tests/golden/build-gate.json。录一次后不再 spawn node，
    // 也不依赖 electron/ 的 build-compatibility 实现。
    #[test]
    fn frozen_node_gate_trace_matches_window_identity_and_errors() {
        let actions = json!([
            ["assert", "main"],
            ["snapshot", "main"],
            ["host", "3.1.71+123"],
            ["verify", "main", " 3.1.71+123 "],
            ["assert", "main"],
            ["snapshot", "other"],
            ["verify", "other", null],
            ["assert", "main"],
            ["verify", "other", "3.1.71+123"],
            ["forget", "other"],
            ["snapshot", "other"],
            ["host", "3.1.71+124"],
            ["assert", "main"]
        ]);
        let cases = json!([
            {"main":"3.1.71+123","actions":actions},
            {"main":"3.1.71+nobuild","actions":[["host","3.1.71+nobuild"],["verify","main","3.1.71+nobuild"],["assert","main"]]},
            {"main":"unknown-version+123","actions":[["host","unknown-version+123"],["verify","main","unknown-version+123"]]},
            {"main":"3.1.71+123","actions":[["host","3.1.71+123"],["verify","main","3.1.71+123"],["verify","main","3.1.71+124"],["verify","main","3.1.71+123"],["assert","main"]]}
        ]);
        let expected: Value = serde_json::from_str(include_str!("../tests/golden/build-gate.json")).unwrap();
        let actual: Vec<Value> = cases
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                let mut gate = BuildIdentity::new(case["main"].as_str().unwrap().into());
                let mut observations = vec![];
                let mut notices = vec![];
                for step in case["actions"].as_array().unwrap() {
                    let value = match step[0].as_str().unwrap() {
                        "host" => {
                            gate.set_host(&step[1]);
                            Value::Null
                        }
                        "verify" => gate.verify(step[1].as_str().unwrap(), &step[2]),
                        "snapshot" => gate.snapshot(step[1].as_str().unwrap()),
                        "forget" => {
                            gate.forget(step[1].as_str().unwrap());
                            Value::Null
                        }
                        "assert" => match gate.assert_writable(step[1].as_str().unwrap()) {
                            Ok(()) => Value::Null,
                            Err(e) => json!({"code":e["code"],"message":e["message"]}),
                        },
                        other => panic!("{other}"),
                    };
                    notices.extend(gate.take_attention());
                    observations.push(json!({"value":value,"notices":notices}));
                }
                json!(observations)
            })
            .collect();
        assert_eq!(json!(actual), expected);
    }
    // 只读集合的「Rust 分类 ↔ 契约快照」对应关系放在 JS 侧核对（electron/tests/ipc-contract.test.cjs
    // 断言 READ_ONLY_CHANNELS === src-tauri/legacy-read-only-channels.json），这里不再 spawn node：
    // 测试只负责 Rust 分类本身。旧实现（electron/）已删除，起 node 读契约会把一个已迁移的检查留成
    // 跨语言依赖；契约在 JS 侧本来就有自己的核对入口。
    #[test]
    fn agent_schema_keeps_native_gate_without_forging_renderer_build() {
        let mut gate = BuildIdentity::new("3.1.74+test".into());
        assert!(gate.negotiate_agent("agent:test", 2).is_err());
        gate.negotiate_agent("agent:test", 1).unwrap();
        assert!(gate.assert_writable("agent:test").is_err());
        gate.set_host(&json!("3.1.74+test"));
        assert!(gate.assert_writable("agent:test").is_ok());
        assert_eq!(gate.snapshot("agent:test")["renderer"], Value::Null);
        gate.set_host(&json!("3.1.74+wrong"));
        assert!(gate.assert_writable("agent:test").is_err());
        gate.set_host(&json!("3.1.74+test"));
        assert!(gate.assert_writable("agent:test").is_err());
    }
    #[test]
    fn readonly_classification_matches_the_frozen_contract_snapshot() {
        let actual: BTreeSet<String> =
            serde_json::from_str(include_str!("../legacy-read-only-channels.json")).unwrap();
        for channel in actual {
            assert!(read_only(&channel));
        }
        assert!(!read_only("data:apply"));
        assert!(!read_only("local-data:secret-set"));
    }
}


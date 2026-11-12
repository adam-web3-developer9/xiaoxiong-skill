//! Application-owned, restart-scoped pairing authority. Only trusted native UI
//! calls decide/revoke; those operations are never dispatched over the socket.
use crate::{valid_token, Error, Result};
use rand::RngCore;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
pub const SCOPES: &[&str] = &[
    "read",
    "edit",
    "generate",
    "export",
    "channels.read",
    "channels.write",
    "channels.credentials.write",
];
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn secret() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn denied(code: &str) -> Error {
    Error::new(code, "Agent authorization is unavailable or expired")
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub session_id: String,
    pub client: Value,
    pub scopes: Vec<String>,
    pub draft_ids: Vec<String>,
    pub expires_at: u64,
    #[serde(skip)]
    token: String,
    #[serde(skip)]
    context: String,
}
impl Grant {
    pub fn allows_draft(&self, id: &str) -> bool {
        self.draft_ids.iter().any(|x| x == "*" || x == id)
    }
    pub fn requires_scope(&self, scope: &str) -> Result<()> {
        if self.scopes.iter().any(|x| x == scope) {
            Ok(())
        } else {
            Err(Error::new(
                "AGENT_SCOPE_DENIED",
                format!("Scope required: {scope}"),
            ))
        }
    }
}
struct Pending {
    public: Value,
    secret: String,
    context: String,
    grant: Option<Grant>,
}
#[derive(Default)]
struct State {
    pending: BTreeMap<String, Pending>,
    grants: BTreeMap<String, Grant>,
}
#[derive(Default)]
pub struct Authorizations {
    state: Mutex<State>,
}
impl Authorizations {
    pub fn request(&self, client: Value, params: &Value, context: String) -> Result<Value> {
        let scopes: Vec<String> = serde_json::from_value(params["scopes"].clone())
            .map_err(|_| Error::new("AGENT_INVALID_REQUEST", "scopes must be an array"))?;
        let draft_ids: Vec<String> = serde_json::from_value(params["draftIds"].clone())
            .map_err(|_| Error::new("AGENT_INVALID_REQUEST", "draftIds must be an array"))?;
        if scopes.is_empty()
            || scopes.iter().any(|s| !SCOPES.contains(&s.as_str()))
            || draft_ids.iter().any(|s| s.trim().is_empty())
        {
            return Err(Error::new(
                "AGENT_INVALID_REQUEST",
                "Unknown scope or empty draft ID",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let poll = secret();
        let code = secret()[..8].to_uppercase();
        let expires = now() + 300_000;
        let public = json!({"requestId":id,"client":client,"scopes":scopes,"draftIds":draft_ids,"userCode":code,"expiresAt":expires,"status":"pending"});
        self.state.lock().unwrap().pending.insert(
            id.clone(),
            Pending {
                public,
                secret: poll.clone(),
                context,
                grant: None,
            },
        );
        Ok(
            json!({"requestId":id,"pollSecret":poll,"userCode":code,"expiresAt":expires,"intervalMs":1000}),
        )
    }
    pub fn poll(&self, id: &str, poll: &str, context: &str) -> Result<Value> {
        let mut state = self.state.lock().unwrap();
        let p = state
            .pending
            .get_mut(id)
            .ok_or_else(|| denied("AGENT_AUTH_EXPIRED"))?;
        if !valid_token(Some(poll), &p.secret) {
            return Err(denied("AGENT_UNAUTHORIZED"));
        }
        if p.public["expiresAt"].as_u64().unwrap_or(0) <= now() || p.context != context {
            return Err(denied("AGENT_AUTH_EXPIRED"));
        }
        if p.public["status"] == "denied" {
            return Err(denied("AGENT_AUTH_DENIED"));
        }
        if let Some(grant) = p.grant.take() {
            let reply = json!({"sessionId":grant.session_id,"token":grant.token,"scopes":grant.scopes,"draftIds":grant.draft_ids,"expiresAt":grant.expires_at});
            if !state.grants.contains_key(&grant.session_id) {
                return Err(denied("AGENT_UNAUTHORIZED"));
            }
            state.pending.remove(id);
            Ok(reply)
        } else {
            Ok(json!({"status":"pending","intervalMs":1000}))
        }
    }
    pub fn authenticate(&self, id: &str, token: &str, context: &str) -> Result<Grant> {
        let mut state = self.state.lock().unwrap();
        state
            .grants
            .retain(|_, g| g.expires_at > now() && g.context == context);
        let grant = state
            .grants
            .get(id)
            .ok_or_else(|| denied("AGENT_UNAUTHORIZED"))?;
        if !valid_token(Some(token), &grant.token) {
            return Err(denied("AGENT_UNAUTHORIZED"));
        }
        Ok(grant.clone())
    }
    pub fn list(&self, context: &str) -> Value {
        let mut state = self.state.lock().unwrap();
        state.pending.retain(|_, p| {
            p.public["expiresAt"].as_u64().unwrap_or(0) > now() && p.context == context
        });
        state
            .grants
            .retain(|_, g| g.expires_at > now() && g.context == context);
        json!({"pending":state.pending.values().filter(|p|p.public["status"]=="pending").map(|p|p.public.clone()).collect::<Vec<_>>(),"grants":state.grants.values().collect::<Vec<_>>()})
    }
    pub fn decide(&self, id: &str, approve: bool, context: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let p = state
            .pending
            .get_mut(id)
            .ok_or_else(|| denied("AGENT_AUTH_EXPIRED"))?;
        if p.context != context
            || p.public["expiresAt"].as_u64().unwrap_or(0) <= now()
            || p.public["status"] != "pending"
        {
            return Err(denied("AGENT_AUTH_EXPIRED"));
        }
        p.public["status"] = json!(if approve { "approved" } else { "denied" });
        if approve {
            p.grant = Some(Grant {
                session_id: uuid::Uuid::new_v4().to_string(),
                client: p.public["client"].clone(),
                scopes: serde_json::from_value(p.public["scopes"].clone()).unwrap(),
                draft_ids: serde_json::from_value(p.public["draftIds"].clone()).unwrap(),
                expires_at: now() + 8 * 60 * 60 * 1000,
                token: secret(),
                context: context.into(),
            });
        }
        let grant = p.grant.clone();
        if let Some(grant) = grant {
            state.grants.insert(grant.session_id.clone(), grant);
        }
        Ok(())
    }
    pub fn revoke(&self, id: &str) {
        self.state.lock().unwrap().grants.remove(id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_requires_ui_and_secret_and_revoke_works() {
        let auth = Authorizations::default();
        let p = auth
            .request(
                json!({"id":"cli"}),
                &json!({"scopes":["read"],"draftIds":["d1"]}),
                "account:1".into(),
            )
            .unwrap();
        let id = p["requestId"].as_str().unwrap();
        let secret = p["pollSecret"].as_str().unwrap();
        assert!(auth
            .poll(id, p["userCode"].as_str().unwrap(), "account:1")
            .is_err());
        assert_eq!(
            auth.poll(id, secret, "account:1").unwrap()["status"],
            "pending"
        );
        auth.decide(id, true, "account:1").unwrap();
        let g = auth.poll(id, secret, "account:1").unwrap();
        let sid = g["sessionId"].as_str().unwrap();
        let token = g["token"].as_str().unwrap();
        let grant = auth.authenticate(sid, token, "account:1").unwrap();
        assert!(grant.allows_draft("d1"));
        assert!(!grant.allows_draft("d2"));
        assert!(grant.requires_scope("generate").is_err());
        assert!(!auth.list("account:1").to_string().contains(token));
        auth.revoke(sid);
        assert!(auth.authenticate(sid, token, "account:1").is_err());
    }
    #[test]
    fn denied_expired_and_revoked_before_poll_never_issue_credentials() {
        let auth = Authorizations::default();
        let make = || {
            auth.request(
                json!({"id":"cli"}),
                &json!({"scopes":["read"],"draftIds":["*"]}),
                "a".into(),
            )
            .unwrap()
        };
        let p = make();
        let id = p["requestId"].as_str().unwrap();
        let secret = p["pollSecret"].as_str().unwrap();
        auth.decide(id, false, "a").unwrap();
        assert_eq!(
            auth.poll(id, secret, "a").unwrap_err().code,
            "AGENT_AUTH_DENIED"
        );
        let p = make();
        let id = p["requestId"].as_str().unwrap();
        let secret = p["pollSecret"].as_str().unwrap();
        auth.state
            .lock()
            .unwrap()
            .pending
            .get_mut(id)
            .unwrap()
            .public["expiresAt"] = json!(0);
        assert_eq!(
            auth.poll(id, secret, "a").unwrap_err().code,
            "AGENT_AUTH_EXPIRED"
        );
        let p = make();
        let id = p["requestId"].as_str().unwrap();
        let secret = p["pollSecret"].as_str().unwrap();
        auth.decide(id, true, "a").unwrap();
        let sid = auth.list("a")["grants"][0]["sessionId"]
            .as_str()
            .unwrap()
            .to_owned();
        auth.revoke(&sid);
        assert_eq!(
            auth.poll(id, secret, "a").unwrap_err().code,
            "AGENT_UNAUTHORIZED"
        );
        let p = make();
        let id = p["requestId"].as_str().unwrap();
        let secret = p["pollSecret"].as_str().unwrap();
        auth.decide(id, true, "a").unwrap();
        let grant = auth.poll(id, secret, "a").unwrap();
        let sid = grant["sessionId"].as_str().unwrap();
        let token = grant["token"].as_str().unwrap();
        assert!(Authorizations::default()
            .authenticate(sid, token, "a")
            .is_err());
        assert!(auth
            .authenticate(sid, token, "different-account-epoch")
            .is_err());
    }
    #[test]
    fn grant_expiry_is_checked_on_every_request() {
        let auth = Authorizations::default();
        let p = auth
            .request(
                json!({"id":"cli"}),
                &json!({"scopes":["read"],"draftIds":["d1"]}),
                "a".into(),
            )
            .unwrap();
        auth.decide(p["requestId"].as_str().unwrap(), true, "a")
            .unwrap();
        let g = auth
            .poll(
                p["requestId"].as_str().unwrap(),
                p["pollSecret"].as_str().unwrap(),
                "a",
            )
            .unwrap();
        let id = g["sessionId"].as_str().unwrap();
        let token = g["token"].as_str().unwrap();
        assert!(auth.authenticate(id, "wrong-token", "a").is_err());
        auth.state
            .lock()
            .unwrap()
            .grants
            .get_mut(id)
            .unwrap()
            .expires_at = 0;
        assert!(auth.authenticate(id, token, "a").is_err());
    }
    #[test]
    fn account_change_and_restart_invalidate() {
        let a = Authorizations::default();
        let p = a
            .request(
                json!({}),
                &json!({"scopes":["read"],"draftIds":[]}),
                "a".into(),
            )
            .unwrap();
        assert!(a
            .decide(p["requestId"].as_str().unwrap(), true, "b")
            .is_err());
        assert!(Authorizations::default()
            .poll(
                p["requestId"].as_str().unwrap(),
                p["pollSecret"].as_str().unwrap(),
                "a"
            )
            .is_err());
    }
}

#[cfg(test)]
pub(crate) fn test_grant(scopes: &[&str], draft_ids: &[&str]) -> Grant {
    Grant {
        session_id: "test-session".into(),
        client: json!({"id":"test","name":"Test","version":"1"}),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
        draft_ids: draft_ids.iter().map(|s| s.to_string()).collect(),
        expires_at: now() + 60_000,
        token: secret(),
        context: "test-context".into(),
    }
}

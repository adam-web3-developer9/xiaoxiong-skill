use crate::*;
pub(crate) fn supports(channel: &str) -> bool {
    matches!(
        channel,
        "account:status"
            | "account:action"
            | "local-data:get-info"
            | "local-data:kv-get"
            | "local-data:kv-set"
            | "local-data:kv-delete"
            | "local-data:settings-snapshot"
            | "local-data:secret-set"
            | "local-data:secret-delete"
            | "local-data:secret-reveal"
            | "local-data:secret-import-legacy"
            | "local-data:secret-status"
            | "local-data:secret-store-state"
            | "local-data:tasks-list"
            | "local-data:execution-logs-list"
            | "local-data:execution-logs-upsert"
            | "local-data:execution-logs-clear"
            | "local-data:drafts-create"
            | "local-data:drafts-duplicate"
            | "data:apply"
            | "task-history:query"
            | "task-history:stats"
            | "task-history:detail"
            | "task-history:facets"
            | "task-history:info"
            | "task-history:latest-terminal"
            | "task-history:sync"
            | "task-history:prune-snapshots"
            | "telemetry:report"
            | "telemetry:settings"
            | "telemetry:clear"
    )
}
impl DesktopApi {
    pub(crate) async fn data(
        &self,
        channel: &str,
        args: &[Value],
        undefined_paths: &[String],
    ) -> Result<Reply> {
        let first = arg(args, 0);
        match channel {
            "account:status"=>Ok(Reply::value(self.account.snapshot())),
            "account:action"=>Ok(Reply::value(self.account.action(&string_arg(args,0),arg(args,1).clone()).await?)),
            "local-data:get-info"=>Ok(Reply::value(json!({"dataRoot":self.root,"dbPath":self.root.join("aistudio.db"),"assetsRoot":self.root.join("media-v4")}))),
            "local-data:settings-snapshot"=>{let mut result=self.runtime("config.settings.snapshot",json!({}),&[])?;result.value=json!({"settings":self.mask_settings(&result.value["settings"])?,"version":result.value["version"]});Ok(result)},
            "local-data:kv-get"=>{
                let namespace=string_arg(args,0);let key=string_arg(args,1);
                if namespace=="director-knowledge"&&key=="library-v1"{return self.director_knowledge();}
                let mut result=self.runtime("config.kv.get",json!({"namespace":namespace,"key":key}),&[])?;
                if namespace=="settings"&&key=="app"&&aistudio_config_domain::truth(&result.value){result.value=self.mask_settings(&result.value)?;}
                Ok(result)
            }
            "local-data:kv-set"=>self.runtime("config.kv.set",json!({"namespace":string_arg(args,0),"key":string_arg(args,1),"value":arg(args,2)}),&[]),
            "local-data:kv-delete"=>self.runtime("config.kv.delete",json!({"namespace":string_arg(args,0),"key":string_arg(args,1)}),&[]),
            "local-data:secret-set"=>self.runtime("secret.set",json!({"key":first,"value":arg(args,1)}),&[]),
            "local-data:secret-delete"=>self.runtime("secret.delete",json!({"key":first}),&[]),
            "local-data:secret-reveal"=>self.reveal_local_config(&string_arg(args,0)),
            "local-data:secret-import-legacy"=>self.runtime("secret.importLegacy",json!({}),&[]),
            "local-data:secret-status"=>self.runtime("secret.status",json!({"keys":first}),&[]),
            "local-data:secret-store-state"=>self.runtime("secret.storeState",json!({}),&[]),
            "local-data:tasks-list"=>self.runtime("task.public",json!({}),&[]),
            "local-data:execution-logs-list"=>self.runtime("executionLog.list",json!({"limit":80}),&[]),
            "local-data:execution-logs-upsert"=>self.runtime("config.logs.upsert",json!({"entry":first}),&[]),
            "local-data:execution-logs-clear"=>self.runtime("config.logs.clear",json!({}),&[]),
            "local-data:drafts-create"=>self.v4("draft.create",json!({"draft":first,"options":{}}),&[]),
            "local-data:drafts-duplicate"=>self.v4("draft.duplicate",json!({"id":string_arg(args,0),"options":arg(args,1)}),&[]),
            "data:apply"=>{
                if first["origin"]["kind"]!="user"{let mut e=error("FIELD_REJECTED","Renderer commands must have user origin");e.name="DataCommandError".into();return Err(e);}
                let paths=undefined_paths.iter().filter_map(|path|path.strip_prefix("/0").filter(|tail|tail.starts_with('/')).map(str::to_owned)).collect::<Vec<_>>();
                let mut result=self.v4("data.apply",first.clone(),&paths)?;result.value=json!({"ok":true,"result":result.value});Ok(result)
            }
            "task-history:query"=>self.runtime("history.query",first.clone(),&[]),
            "task-history:stats"=>self.runtime("history.stats",first.clone(),&[]),
            "task-history:detail"=>self.runtime("history.detail",json!({"id":first}),&[]),
            "task-history:facets"=>self.runtime("history.facets",first.clone(),&[]),
            "task-history:info"=>self.runtime("history.info",json!({}),&[]),
            "task-history:latest-terminal"=>self.runtime("history.latestTerminal",json!({"draftId":first}),&[]),
            "task-history:sync"=>{let mut result=self.runtime("history.backfill",json!({}),&[])?;result.value=json!({"archived":result.value});Ok(result)},
            "task-history:prune-snapshots"=>self.runtime("history.pruneSnapshots",json!({"beforeDayKey":first}),&[]),
            "telemetry:report"=>self.runtime("telemetry.report",first.clone(),&[]),
            "telemetry:settings"=>self.runtime("telemetry.settings",first.clone(),&[]),
            "telemetry:clear"=>{let mut result=self.runtime("telemetry.clear",json!({}),&[])?;result.value=json!({"success":true});Ok(result)},
            _=>Err(error("DESKTOP_CHANNEL_UNIMPLEMENTED",format!("Public desktop channel has no native implementation yet: {channel}"))),
        }
    }
}

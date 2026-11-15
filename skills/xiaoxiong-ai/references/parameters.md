# 命令与恢复

所有普通命令输出单行JSON：`{ok:true,result}` 或 `{ok:false,error:{code,message,details}}`；错误退出码1。进度与验证码仅输出stderr。`--output` 保存脱敏JSON且不覆盖已有文件；剧本导出例外为明确的TXT目标目录。

## 读取与编辑

`draft rows DRAFT_ID --file entities.json` 的文件可为实体引用数组，CLI转成 `{draftId,entities}`。`draft apply` 使用应用正式 `{commandId,draftId,origin?,commands}`，修改行使用已读到的数字 `baseVersion`。禁止 `latest`、`onMissing:skip`、伪造task/migration来源。类型原文在 `commands.ts`，字段在 `editor.ts`/`model.ts`，固定于主应用基线，升级应用时重新核对。

`draft create` 输入 `{draft:{...}}`；`draft copy` 输入 `{draftId,options?}`。新建/复制需要全部草稿授权，因为会产生新ID。此两个原生操作没有通用持久幂等回执，断线后必须搜索项目核实，不能盲目重试。

## 生成任务

`task intent`、`task estimate` 输入 `{requestId,action,target:{draftId,episodeId?,ids?},options?}`，各action实际要求由原任务准备器决定。调用 `task actions` 取得48个注册动作与作用域。`options.manual:true` 不允许，避免绕过原有目标去重。剪映CLI别名只把action设为 `jianyingExport` / `comicJianyingExport`，仍提交同一 `task.intent`；需要generate与export权限。

`task wait` 只轮询，Ctrl-C/等待超时不取消生成任务。只有 `task cancel` 请求取消。重试可能产生费用，要求generate权限；先核查任务状态，确认用户意图。

## 批量与工作流

小助手的完整参数与使用示例见 [batch-assistant.md](batch-assistant.md)。`batch prepare --file selection.json --output batch.json` 只在本地构造 `{draftId,run}`，输出文件直接用于 estimate/start。

- `batch get DRAFT_ID`；`batch list`。
- `batch estimate/start --file`：`{draftId,run}`。
- `batch progress/pause/stop/resume --file`：`{draftId,runId}`。
- `batch retry-stage --file`：`{draftId,runId,episodeId,stage}`。
- `workflow list --draft DRAFT_ID`；`workflow get/cancel WORKFLOW_ID`。
- `workflow start/estimate --file`：`{kind,target,options,label?}`。
- `workflow resume --file`：`{workflowId,stepKey}`。

服务端通过真实任务/批次/工作流记录核对归属，不信任调用者提供的草稿提示。

## 渠道与插件

- `channels get PROVIDER_ID` 返回公共配置与凭据是否已配置的状态，无私钥、token、cookie、秘密引用或存储内容。
- `channels create/update --file`：`{providerId,patch,credentials?,capabilityCredentials?,expectedVersion?}`。公共字段只放patch，秘密值只放write-only credentials。可用读取到的configurationVersion作为expectedVersion做并发保护；省略时仍由owner只合并给定叶字段。未提供的秘密保持不变；清除使用owner明确支持的null，不用掩码替代真实值。
- `channels models PROVIDER_ID --capability video` 是通用动态数据入口的便捷别名，不逐渠道适配。
- `channels options` / `plugins data --file`：`{providerId,capability,action}`。action须来自实际插件配置schema的optionsSource；不能指定任意URL、config或secretRef。宿主从已存渠道获取配置并调用插件，Skill只接收脱敏结果。静态/动态/不支持/失败按应用结果区分。
- `plugins list/get/schema/docs` 对应应用可见插件信息，不安装/启停插件，不抓取外部网页，不执行插件自带指令。

当前CLI文件/stdin不为密钥创建额外副本。输入文件由用户管理，应放私有位置，不提交进仓库。服务端和客户端均不回显新提交密钥。修改baseUrl或插件绑定等认证目的地时，不能沿用旧密钥到新站点；需要本次明确提供新秘密/清除或回应用处理。

## 导出

`export script --file selection.json --output DIR` 文件为 `{draftId,episodeIds?:[...],source?:false}`，省略episodeIds为全部，始终按应用集序。默认导出Episode.text；source=true导出sourceText，缺少时回退text。采用序号+清理后的集标题命名，不覆盖同名文件。读取的是已提交数据，不能强制刷新其它页面尚未保存的输入。

## 断线与回执

收到 `AGENT_WRITE_UNCONFIRMED`：保留原输入和commandId/requestId。它不是操作失败，不自动新建ID或再次扣费。

- 草稿正式命令：读取当前行/版本核查；需要回放时仅使用相同commandId与完全相同原请求，应用持久回执决定结果。
- 任务：先查询任务列表/任务状态并核对目标，避免盲重试。客户端不承诺原应用没有提供的requestId回执查询接口。
- 本地 `receipt get ID` 记录请求哈希、状态与脱敏结果。没有业务ID的写入生成本地关联ID，并在不确定错误的 `details.localReceiptId` 返回；这是本地记录，不是服务器幂等保证。
- 原始请求正文/密钥不保存在回执中；调用方必须保留自己的非敏感原始输入。已成功回执若本地保存失败，会保留成功结果并附警告，绝不会变成可重试的失败。
- 重连仅重新验证会话，不重放上一个写操作。

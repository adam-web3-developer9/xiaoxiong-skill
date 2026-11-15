# 目录比对与小助手接入验证（2026-11-16）

## 同步基线

比对对象：当前 `D:/workspace/xiaoxiong-skill`、下载目录 `D:/Downloads/xiaoxiong-agent-source/xiaoxiong-agent`，及本机 `.agents/skills/xiaoxiong-ai`、`.claude/skills/xiaoxiong-ai`。

- 当前仓库和下载目录均为 `286677dbbc9dfe0e736b72e21f0cac46160d8804` 基线、干净工作树。当前仓库 fetch 后与 origin/main 一致。
- 117 个共同文件忽略 CRLF/LF 后内容全部相同，不能按文件复制时间判断版本新旧。
- 下载目录多两份被忽略的 `.log`，其正文与仓库 `docs/validation/native-build-gate-tests.txt`、`native-protocol-tests.txt` 相同，没有额外代码。
- 两个本机安装 Skill 的 8 个文件都与当前仓库 Skill 逐字节一致；`.codex/skills/xiaoxiong-ai` 不存在。

因此无旧目录内容需要覆盖当前仓库。本次以当前 Git 基线补齐小助手的可用参数和指南；不覆盖 AIStudio 既有工作区改动，不重新应用历史 overlay。

## 实现与源码核对

- 新增本地 `batch prepare --file selection.json --output batch.json`，输出正式 `{draftId,run}`，可直接交给已有 batch.estimate/start。
- run 构造依据 AIStudio `src/core/batch/pipeline.ts:createBatchRun`：固定八阶段、选中 idle、未选中 skipped、按集序保留 ID、四种分镜模式、关闭故事板时排除 imagePrompt/shotImage。
- 独立复核通过 Node 24 原生 TypeScript 导入应用原函数，四模式与新 builder 输出 deepEqual（排除随机 runId、时间戳和由宿主覆盖的 retryCount）。
- 应用 start 会用当前设置冻结自动重试次数；省略 storyboardEnabled 时宿主按 true 处理，指南明确不将此描述为继承项目关闭设置。
- MCP 原有 batch estimate/start 补齐 run JSON schema；其余生命周期调用继续使用同一官方路由，没有新增调度器或应用方法。
- Skill 新指南覆盖分集/阶段选择、现有创作保留、估算与成本未知、开始/暂停/继续/停止/阶段重试和未知结果查询。

## 当前验证

普通 CLI 使用现有 read 授权连接运行中的应用，未读取或输出凭据：

- `auth status`：Agent protocol 2、schema 1，main/host 同为 `3.1.79+1794753562946`，mismatched=false。
- `batch list/get`：真实运行中的同一批次，11 个分集，选择 assets/assetImages/storyboard/videoPrompt/shotVideo。
- `batch progress`：返回真实状态/数量，采样 done=899、total=911、percent=99；这是当时快照，不是完结验收。
- `prepareBatch` 以本批次首集、shotVideo、max15、storyboardEnabled=false 构造新估算载荷；通过普通 `AgentClient.call('batch.estimate')` 接受，返回 taskCount=0、reusedCount=0、credits=0、unknownPriceCount=0、unresolvedStages=[]。只证明此样本已无待估算任务，不能推广为其他生产免费。
- `npm test`：33 项，31 通过、0 失败、2 跳过。跳过项为未配置的 test-only Rust fixture、环境 EACCES 禁止的真实 socket bind。
- 新增 4 项覆盖：批次选择与初始化、四模式/无板步骤、离线 CLI 文件直接用于 estimate/start、MCP 完整 schema 和同 runId 生命周期转发。
- `npm run check`：21 个模块语法检查通过。
- `python -X utf8 .../skill-creator/scripts/quick_validate.py skills/xiaoxiong-ai`：通过。Windows 默认 GBK 读取不适用于 UTF-8 Skill，显式使用 UTF-8。
- `git diff --check`：通过。

本次没有对真实批次执行 start/pause/resume/stop/retry，也没有付费生成、打包或部署；控制路由使用测试客户端验证转发。应用公共进度投影尚不完整提供环境原因、停止统计和分镜配置，Skill 如实保留未知边界。

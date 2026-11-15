# 小助手：按剧集批量生产

小助手对应应用的 `batch.*`，由同一个宿主调度器推进，应用关闭后不会由此 CLI 继续调度。`workflow.*` 是另一组持久工作流接口，不能用 workflowId 代替 batch runId。

先读 `xiaoxiong capabilities`，确认实际提供所需方法。查询/估算需要 `read`，开始/继续/重试需要 `generate`，暂停/停止需要 `edit`；授权范围还须包含目标草稿。只有确实要执行时才按已有授权流程扩展 scope。

## 选择剧集和步骤

通过 `draft list/search/get` 读取目标草稿与已保存的分集 ID，保持应用集序。先用 `batch get DRAFT_ID` 查看是否已有一轮；用户说“继续”时使用原 runId 的 `batch resume`，不要重新 prepare/start。

步骤使用以下键，按应用顺序执行；只选择用户要做的步骤：

| 键 | 功能 |
| --- | --- |
| `assets` | 提取资产，跨集串行复用已有资产 |
| `assetImages` | 资产生图 |
| `storyboard` | 从剧本切分分镜 |
| `imagePrompt` | 生成故事板提示词 |
| `videoPrompt` | 生成视频提示词 |
| `shotImage` | 分镜生图 |
| `shotVideo` | 生成视频 |
| `watermarkRemove` | 视频去水印，另行计费 |

外部 Skill 已交付剧本、分镜或提示词时，保留这些结果。例如只根据已保存提示词生成视频，选择 `shotVideo`；不要顺手选 `storyboard` 或提示词重推。未勾选的前置数据仍须已经具备，缺失时由宿主返回等待/阻塞事实。应用抽屉通常默认前七步；CLI prepare 要求明确给出 stageKeys，不自动勾选去水印。

## 本地准备批次文件

配置文件 `selection.json` 示例（替换为读到的真实 ID）：

```json
{
  "draftId": "DRAFT_ID",
  "episodes": [
    { "id": "EPISODE_ID_1", "title": "第一集" },
    { "id": "EPISODE_ID_2", "title": "第二集" }
  ],
  "stageKeys": ["shotVideo"],
  "storyboardMode": "max15",
  "storyboardEnabled": false
}
```

```sh
xiaoxiong batch prepare --file selection.json --output batch.json
xiaoxiong batch estimate --file batch.json
```

`prepare` 只在本地生成 `{draftId,run}`，不连接应用、不生成任务、不收费。`--output` 写出的 JSON 可直接交给 estimate/start；省略时返回普通 `{ok:true,result}`。开始后保留该文件及其中的 runId；不要用新的 prepare 代替结果未知的开始请求。

`storyboardMode` 支持 `max15`、`max30`、`greedy15`、`greedy30`，省略使用 **CLI 的 max15 默认**。它不读取应用的项目/全局默认；要沿用界面选择，先在应用确认当前有效方式并填写。模式对应 `storyboardDuration=15/30`、`storyboardForceMax=true` 和 `storyboardGreedy=false/true`。

`storyboardEnabled` 请按应用实际故事板模式填写：明确 false 时，prepare 同界面一样排除 `imagePrompt`、`shotImage`。省略时宿主按有故事板模式处理，并不继承项目/全局的关闭选择；当前关闭故事板时应显式传 false。自动重试次数、模型、渠道、并发、账户权限及生成准备由宿主当前设置决定，prepare 不复制或覆写这些设置。

估算不是完整费用保证：后续分镜数量等尚未产生时，部分费用可能未知。按用户已有授权说明实际估算与范围，再执行一次：

```sh
xiaoxiong batch start --file batch.json
xiaoxiong batch get DRAFT_ID
xiaoxiong batch list
```

开始返回的是宿主接受的状态，不等于生成完成。已保存提示词仍经过应用现有素材匹配、引用转换和渠道提交路径。

## 进度和控制

将本轮的真实 runId 保存到 `run.json`：

```json
{ "draftId": "DRAFT_ID", "runId": "BATCH_RUN_ID" }
```

```sh
xiaoxiong batch progress --file run.json
xiaoxiong batch pause --file run.json
xiaoxiong batch resume --file run.json
xiaoxiong batch stop --file run.json
```

- 暂停：不再安排新任务，已入队任务继续执行。
- 继续：同一轮交回宿主继续，不创建新批次。
- 停止：宿主停止调度并清理本项目尚未提交的任务；已提交上游的任务保留收取。不能承诺所有上游立即取消或退款。
- 进度：`batch progress` 读取各集阶段状态和数量，`batch get` 读取阶段 taskIds 后用 `task get TASK_ID` 查询失败详情。`completed` 表示这一轮已结束，还需检查阶段是否 failed/blocked/skipped，不能直接报告全部生成成功。

只重试指定阶段，另存 `retry.json`：

```json
{
  "draftId": "DRAFT_ID",
  "runId": "BATCH_RUN_ID",
  "episodeId": "EPISODE_ID_1",
  "stage": "shotVideo"
}
```

```sh
xiaoxiong batch retry-stage --file retry.json
```

重试可能生成新任务并计费。以宿主返回和任务记录判断是否接受/执行，不把请求返回当成已出片。传输返回 `AGENT_WRITE_UNCONFIRMED` 时先 `batch get/progress` 核对原 runId，不自动重发或新建一轮。

## MCP 与完整 run 结构

MCP 对应 `xiaoxiong_batch_estimate/start/get/list/progress/pause/stop/resume/retryStage`，参数与 CLI 相同。`prepare` 是本地 CLI 工具，不是应用方法或 MCP 工具。MCP 的 estimate/start 已公开完整 run schema；可使用 prepare 结果，或按如下结构构造：

- 外层 `{draftId,run}`；run.draftId 必须相同。
- run 包含唯一 `runId`、`status:"running"`、毫秒时间戳 `createdAt/updatedAt`、按集序的 `episodeIds`、所选 `stageKeys`、分镜模式字段。
- `run.episodes` 以所选分集 ID 为键，每项 `{episodeId,title,stages}`。
- 每集 stages 包含上述全部八个键。选中阶段 `{status:"idle",taskIds:[],attempt:0}`，未选中阶段 `{status:"skipped",taskIds:[],attempt:0}`。

这些是新一轮的初始状态。开始后使用应用返回的状态，不手改 taskIds、attempt、version 或调度进度。现有 v2 公共投影不保证返回分镜配置、环境阻塞原因或停止统计；缺失信息保持未知，不据空对象推断环境就绪。

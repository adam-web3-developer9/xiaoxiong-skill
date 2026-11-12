# 外部创作的最小交付约定

外部 Skill 决定剧本、分镜、节奏和提示词正文。小熊AI负责保存数据、匹配资产素材、上传及提交任务。无需套用应用内置创作模板，也无需先读取完整流程模板库。

## 剧本：推荐格式，不是强制排版

剧集保存为 `Episode` 的 `id`、`title`、`text`。推荐正文使用一行一个标签，方便应用区分画面与发声；外部 Skill 已有排版可以保留，不因没有这些标签拒绝导入或自动改写：

```text
【场景头】1-1 老书店 夜 内
【画面】窗外下雨，柜台上亮着旧台灯。
【动作】林夏把旧信放在柜台上。
【对话】林夏：这封信，是你寄给我的吗？
【动作】陈伯擦眼镜的手停住。
【对话】陈伯：我等你来问，已经等了十年。
```

需要时可用 `【内心OS】角色：正文`、`【旁白】正文`、`【画外音】角色：正文`、`【音效】正文`。这是便于识别的建议，不要求每种标签都出现，也不改变外部 Skill 的内容。

## 分镜：剧本内容和秒数必填

每条分镜提供 `scriptContent`（本条的剧本内容）和 `duration`（正数，单位秒）。调用方还要分配稳定 `id`、关联目标 `episodeId`；这些是保存定位信息。最小示例：

```json
{
  "id": "external-shot-1",
  "episodeId": "episode-1",
  "scriptContent": "林夏把旧信放在柜台上，问：这封信，是你寄给我的吗？",
  "duration": 8
}
```

`cutPlan`、`directorPlan`、`visualBeatPlan`、`storyboardSplitData`、格数规划等应用内部流程数据均可不提供。没有这些字段，也不要为了导入而编造、补齐或重新生成。外部 Skill 已提供且正式字段支持的数据可以保留。

## 提示词：正文自定，引用资产使用应用标记

故事板提示词写入 `storyboardBoards[].imagePrompt`，以 `boardId` 标识板号；视频提示词写入本条分镜的 `videoPrompt`。正文的语言、章节、镜头写法和故事板排版由外部 Skill 决定。

**需要应用匹配并上传的资产，在提示词对应位置使用 `{类型:资产名称}`。** 常用写法：

| 用途 | 写法 |
| --- | --- |
| 角色图片 | `{角色:林夏}` |
| 场景图片 | `{场景:老书店}` |
| 道具图片 | `{道具:旧信}`（也支持 `{物品:旧信}`） |
| 视觉符号图片 | `{符号:家族徽记}` |
| 视频中的角色音色 | `{音色:林夏音色}` |

名称对应草稿资产的实际名称或已登记别名；先通过 `draft get` / `draft rows` 读取资产和候选素材。引用图片的资产需要有实际图片素材，引用音色需要有相应音频或音色设置；只有资产文字描述不会变成已上传文件。

应用按引用匹配素材，按当前渠道整理并上传，然后把这些标记转换成真实参考编号。不要自行猜测 `图片1`、`参考图1`、`<Picture 1>` 或上传地址；编号由实际提交的素材顺序决定。可选 `requiredAssetIds` 也会参与素材选择；正文里的标记用于明确对象与附件的对应关系。

图片任务使用视觉素材；音色标记用于视频，不要把音频当作图片参考。故事板与视频可以在各自提示词中标明需要的资产；实际附件还受分镜所需资产清单、已绑定素材及渠道能力影响。外部无需填写 `assetReferenceBindings`，应用生成实际绑定；既有渠道提交适配照常执行，超出渠道字数时仍按应用现有精简流程处理。

最小提示词示例（假设草稿已有对应图片素材）：

```json
{
  "id": "external-shot-1",
  "episodeId": "episode-1",
  "scriptContent": "林夏把旧信放在柜台上，抬眼询问信的来历。",
  "duration": 8,
  "storyboardBoards": [
    {
      "boardId": 1,
      "imagePrompt": "雨夜，{场景:老书店}内，{角色:林夏}站在木柜台前，把旧信放在台灯旁。采用本次创作指定的构图。"
    }
  ],
  "videoPrompt": "8秒。{角色:林夏}在{场景:老书店}柜台前放下旧信，抬眼问：这封信，是你寄给我的吗？保留自然动作与雨声。"
}
```

示例只说明保存位置和引用写法，不规定镜数、宫格数、节拍结构或统一时长。多板时分别保存每板的 `boardId` 与 `imagePrompt`；视频使用的已选故事板素材由应用绑定，不自行编造 `{故事板:…}` 资产类型。

## 标准字段参考：按需使用

需要保留更完整的数据时，读取随 Skill 附带的字段定义；这些是可选扩展，不会让最小分镜多出必填项：

| 要读取的格式 | Skill 内的参考 |
| --- | --- |
| 剧集、场景、分镜正文与图片/视频提示词字段 | [editor.ts](editor.ts) 的 `Episode`、`ScriptScene`、`EditableShot` |
| 资产、分镜、图板、切分计划和连续性字段 | [model.ts](model.ts) 的 `Asset`、`Shot`、`ShotStoryboardBoardImage`、`ShotCutPlan` |
| 创建、修改与版本号的命令外壳 | [commands.ts](commands.ts) 的 `DataCommand`、`DraftApplyRequest` |

这些文件是字段阅读参考；实际可用操作以已连接应用的能力目录为准。直接读取现有数据用 `draft get` / `draft rows`。不把应用内部字段格式变成外部提示词正文模板。

## 保存和提交

通过正式 `draft apply` 使用 `insertEpisodes` / `insertShots` 创建，或用当前数字 `baseVersion` 执行 `patchEpisode` / `patchShot` / `patchShotBoard` 修改。已有提示词不要用 `imagePrompt` / `videoPrompt` 生成动作覆盖。

用户要求实际生成时，从 `task actions` 中选择 `shotImage`（用已存故事板提示词生图）或 `shotVideo`（用已存视频提示词生视频）。例如单条故事板任务：

```json
{
  "requestId": "external-shot-image-001",
  "action": "shotImage",
  "target": {
    "draftId": "draft-1",
    "episodeId": "episode-1",
    "ids": ["external-shot-1"]
  },
  "options": {}
}
```

生成视频时使用另一个固定 `requestId`、`action: "shotVideo"` 和相同目标分镜。素材选择、上传和渠道请求均归应用；任务授权、回执与等待规则见 [operations.md](operations.md)。

# 小熊AI Agent Bridge

独立新项目：小熊AI操作 CLI、通用 Agent Skill、Claude/Codex/OpenCode 等工具可用的 MCP，以及主应用真实接入补丁。不会启动第二套数据库或任务宿主。

源码仓库：`git@github.com:adam-web3-developer9/xiaoxiong-skill.git`。CLI/Skill/MCP 通过应用官方 Agent v2 接口工作。2026-11-16 已用普通 Windows CLI 连接运行中的小熊AI 3.1.79，验证小助手批次查询、进度及本地构造批次的真实估算，见 [小助手接入验证](docs/validation/batch-assistant-20261116.md)。3.1.75 的文本读写与真实智能分镜历史测试见 [旧版应用记录](docs/validation/live-app-20261112.md)，当时使用诊断传输。`integration/aistudio` 保留针对 `aistudio@5423be21112b61b0728cf10a0bc9883e0e308581` 的原始接入补丁，历史源码验证边界见 [验证记录](docs/verification.md)。

## 三步使用

1. 正常构建并启动已支持 Agent v2 授权的小熊AI。只有仍处于上述旧基线的开发副本才使用 `integration/aistudio` 接入补丁；不要将旧补丁覆盖到已升级的应用。
2. 在此项目运行 `node bin/xiaoxiong.mjs auth login --scopes all`。小熊AI出现“Agent连接”面板，核对验证码与权限，点击允许。账户登录和激活仍在应用内完成，不复制账号密码。`all` 明确包含生成、导出、渠道修改和密钥只写权限；只读可用 `--scopes read,channels.read`。
3. 运行 `node bin/xiaoxiong.mjs draft list`。需要非默认数据目录时加 `--data-root /path/to/aistudio-data`。应用在标准临时目录发布无凭据发现标记，支持自定义D盘/便携数据目录；多个运行实例会要求显式选择。标记不可用时可用 --data-root 指定。

环境：Node.js 22 或以上；CLI/MCP 无第三方运行依赖，不用 npm install。可在用户明确选择的环境通过 `npm link` 注册 `xiaoxiong` 命令，也可以一直使用 `node /absolute/path/bin/xiaoxiong.mjs`。

## 授权与存储

- CLI 提出申请，**只有应用原生可信窗口能批准**。验证码只是供人核对，不能兑换访问令牌。
- 权限：read、edit、generate、export、channels.read、channels.write、channels.credentials.write；可限制草稿ID，`*` 表示全部草稿，`--drafts none` 表示不授予任何草稿权限。
- 已批准会话可跨CLI进程复用，当前授权8小时有效，应用重启、账户变更或主动撤销后重新授权。撤销阻止新操作，不自动取消已接收任务。
- 连接凭证与应用账户凭证分离，不出现在 stdout/MCP/日志。客户端保存在本机用户状态目录，POSIX权限600；不引入钥匙串依赖。此机制不隔离同一OS账户下恶意软件。
- v2不读取旧连接token。原v1开发入口默认关闭，仅 `AISTUDIO_AGENT_LEGACY_V1=1` 显式开启兼容。开启后旧入口不受v2草稿授权管理，仅适合可信开发环境。

## 已接入的能力

- 账户状态、草稿列表/搜索/读取/指定行读取/创建/复制。
- 通过正式 data:apply 修改剧本、分集、场景、资产、分镜、故事板、媒体选择和画布，保留数字版本、字段冲突和命令回执。
- 生产任务注册表48类动作，包括剧本分析/改写/评分、资产、分镜图、视频、音色/配音、案例处理、去水印及剪映导出；参数和渠道执行由应用原有任务准备器负责。
- 任务估算、提交、查询、等待、取消、重试、重新检查；批量流程和工作流查询、启动、暂停/停止/恢复及既有阶段重试。
- 渠道公共配置读写；新密钥只写；修改公共字段保留已有密钥；改变认证目的地不得把旧密钥隐式带到新站点。
- 插件列表、版本、来源、状态、能力、配置字段、说明/文档链接及配置动态展示数据。通过应用统一插件宿主读取，不为不同插件编写Skill适配器；插件不支持的动态动作如实报错。
- 剧本按所选集导出TXT；剪映/漫剧剪映通过应用正式任务。
- 多Skill发现/检查/按需读取/本地安装、结构化创作结果转正式命令示例。

能力目录以 `xiaoxiong capabilities` 返回值为准。调用存在不等于供应商已实测；48个动作并非48次真实付费测试。

## Skill 接入

把 `skills/xiaoxiong-ai` 安装到对应工具的 Skill 目录，例如 `.claude/skills/xiaoxiong-ai`（Claude Code）、`.agents/skills/xiaoxiong-ai`（Codex）或 `.opencode/skills/xiaoxiong-ai`（OpenCode）：

```sh
node bin/xiaoxiong.mjs skills install skills/xiaoxiong-ai --target /your/project/.opencode/skills/xiaoxiong-ai
```

这是本地复制，不运行第三方脚本。其他创作Skill可并用，操作Skill负责通过应用保存结果。Skill能导入与所有依赖能运行是不同状态，见 [Skill管理](docs/skills.md)。

外部创作采用 [最小交付约定](skills/xiaoxiong-ai/references/content-format.md)：每条分镜提供剧本内容和秒数即可，内部切分数据可缺省；剧本排版只作推荐，故事板/视频提示词由外部 Skill 编写，只需按应用语法引用要上传的资产。

## 可选 MCP

MCP 使用通用 stdio 协议，可供 Claude、Codex、OpenCode 等支持该协议的客户端接入。各客户端配置格式不同；以下是 OpenCode 配置示例（将绝对路径替换为真实目录）：

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "xiaoxiong": {
      "type": "local",
      "command": ["node", "/absolute/path/xiaoxiong-agent/bin/xiaoxiong.mjs", "mcp"],
      "enabled": true
    }
  }
}
```

先在同一用户环境完成CLI授权。MCP复用相同权限与正式路由，stdio不开放HTTP端口；没有任意IPC、SQL、shell或读取私钥工具。只需Skill时不用MCP。

## 常用命令

小助手现可用 `batch prepare` 将剧集和步骤选择转换成完整批次文件，然后估算、启动和控制同一轮。参数和暂停/停止语义见 [小助手指南](skills/xiaoxiong-ai/references/batch-assistant.md)。

```sh
xiaoxiong auth status
xiaoxiong account status
xiaoxiong draft list
xiaoxiong draft search 项目关键词
xiaoxiong draft get DRAFT_ID
xiaoxiong draft rows DRAFT_ID --file entities.json
xiaoxiong draft apply --file commands.json
xiaoxiong task actions
xiaoxiong task estimate --file intent.json
xiaoxiong task intent --file intent.json
xiaoxiong task wait TASK_ID
xiaoxiong task retry TASK_ID
xiaoxiong batch prepare --file selection.json --output batch.json
xiaoxiong batch estimate --file batch.json
xiaoxiong batch start --file batch.json
xiaoxiong batch get DRAFT_ID
xiaoxiong batch progress --file run.json
xiaoxiong channels list
xiaoxiong channels models PROVIDER_ID --capability video
xiaoxiong channels create --file channel.json
xiaoxiong plugins list
xiaoxiong plugins get PLUGIN_ID
xiaoxiong plugins schema PLUGIN_ID
xiaoxiong plugins docs PLUGIN_ID
xiaoxiong plugins data --file plugin-options.json
xiaoxiong export script --file selection.json --output ./exported-scripts
xiaoxiong export jianying --file export-intent.json
xiaoxiong receipt get REQUEST_ID
```

密钥仅从JSON文件或 `--file -` 的stdin输入，不放命令行参数、对话、配置示例或日志。命令输入细节见 [命令与恢复](docs/commands.md)，接口基线类型见 [commands.ts](docs/contracts/commands.ts)。

## 开发检查

```sh
npm test
npm run check
node --test integration/aistudio/overlay/src/core/agent-authorization/authorization.test.mjs
```

默认测试会明确跳过被宿主禁止的真实socket监听，并分别测试真实客户端的内存传输逻辑。原生Rust与UI验证步骤和限制见验证记录。源码检查不触发模型生成、部署或应用发布；实际生成通过用户授权的应用任务执行。

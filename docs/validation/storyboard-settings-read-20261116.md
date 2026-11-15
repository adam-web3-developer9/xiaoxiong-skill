# 分镜方式读取补齐（2026-11-16）

原因已在当前 AIStudio 宿主源码确认：

- `batch.get` 经 `orchestration_view` 白名单投影，原白名单没有分镜配置字段。
- `draft.get` 原样返回草稿覆盖值，不会删除时长和贪心字段；但继承全局时项目覆盖本就可能没有它们，因此接口缺少有效设置视图，不能据此判断项目未设置。

当前应用源码已补：

- `draft.get.effectiveGenerationSettings.storyboardTargetDuration/storyboardGreedy`，由配置 owner 合并项目覆盖与全局设置；原 `globalGenSettings` 不变。
- 批次公开投影保留记录中的 `storyboardDuration/storyboardForceMax/storyboardGreedy/storyboardEnabled`，不按当前项目值补写历史批次。

CLI 无返回字段白名单，无须修改传输或本地 prepare 默认。Skill 小助手指南补充新字段、继承语义，以及旧应用缺字段保持未知的要求。`integration/aistudio` 是旧基线历史补丁，本轮不覆盖或重制。

验证：配置 owner 定向1项、Agent 读取路由1项、原秘密投影1项、原授权/撤销1项通过，修改文件 diff 检查通过。详见应用 `docs/features/agent-storyboard-settings-read-20261116.md`。未改变 CLI 代码，不重复运行无关业务测试。

运行版尚未更新：普通 CLI 连接的构建号为 `3.1.79+1794759523195`，batch.list 成功，全量 draft.get 本次超时。没有写项目数据、发起生成、构建发布或重启应用。源码测试不代表运行中的接口已经补齐。

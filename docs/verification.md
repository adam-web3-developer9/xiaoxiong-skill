# 验证记录与交付边界

基线：AIStudio main `5423be21112b61b0728cf10a0bc9883e0e308581`，2026-11-12。本项目没有推送、部署、启动用户应用、读取生产账户文件或真实付费调用。

## 已实际通过

- Node CLI/SDK/MCP/Skill/导出测试：设置原生测试fixture后 **26通过，1显式跳过，0失败**。`docs/validation/node-tests.txt`。包含方法目录与应用45个业务方法的精确契约检查、文件/stdin输入、私钥输出/错误/本地回执脱敏、断线不重交、重连必须resume、成功回执落盘失败不误报失败、第三方Skill资源读取/复制边界。
- **跨语言契约**：Node客户端与真正Rust Session/Authorizations/v2_routes通过stdio测试连接，完成配对、认证恢复、能力、按草稿授权读写、越权拒绝、渠道新密钥不回显和注销。业务Host是明确测试夹具，自动批准仅位于 `integration/aistudio/fixtures/protocol_stdio.rs`，不会接入正式应用。它验证两端协议一致，不等于桌面端到端。
- 所有Node模块语法检查通过。CLI help和Skill inspect均能实际运行。
- 应用授权UI：3个纯逻辑/接线测试通过；真实React18/antd5/AppDrawer DOM交互与局部TypeScript检查通过。原生调用与overlay协调器在该测试里隔离，覆盖自动展示、关闭/重新打开、连点批准只发一次、授权显示、撤销、过期禁用。详细日志/命令见validation目录。
- Rust Agent真实crate与真实config/task/domain依赖：31个定向测试通过，包含48个task action路由验证；不是48个真实供应商生成。BuildIdentity构建闸门3项通过。见 `integration/aistudio/VALIDATION.md` 与原生测试日志。
- 主应用补丁：标准git apply检查、精确基线SHA检查、一次性副本应用、重复应用、拒绝覆盖其他本地改动已测。最终manifest覆盖全部集成文件。
- 渠道配置owner真实crate与临时SQLite **16项通过**，production动态输出投影 **2项通过**；覆盖叶级更新、真实事务回滚/并发、版本冲突、自定义writeOnly秘密、继承目的地和null清除回归。见 `integration/aistudio/docs/channel-owner-validation.md`，未用受控Host代替持久化验证。

## 明确未通过或未运行

- **当前云环境禁止本地socket监听**，Node与Rust真实Unix socket测试返回EPERM。Node显式skip，Rust单独保留失败记录后定向运行排除它；没有冒称通过。Windows named pipe也未实测。
- **完整Tauri/安装包构建和真实窗口端到端未验收**。UI局部编译/DOM、Rust协议/配置owner编译均不代表整个应用已构建。主应用完整依赖图及平台系统库需相应环境。
- 原旧golden集成测试编译成功，但执行引用基线已删除的 `electron/agent-data-routes.cjs`，返回MODULE_NOT_FOUND；没有恢复退役Electron代码制造通过。
- 没有运行真实插件动态接口、供应商文本/图/视频/配音生成、付费任务、实际剪映导出或真实编辑器并发刷新。已有正式路由和事件路径已接线，真实体验验收尚需运行应用。
- 没有安装到真实OpenCode/Codex用户配置，仅验证标准stdio MCP协议与Skill包。没有声明所有任意Skill脚本可执行。

## 复跑命令

```sh
npm test
npm run check
node --test integration/aistudio/overlay/src/core/agent-authorization/authorization.test.mjs
```

跨语言fixture：在应用Agent crate中把 `fixtures/protocol_stdio.rs` 复制到测试examples目录，`cargo build --example protocol_stdio`；然后：

```sh
XIAOXIONG_NATIVE_FIXTURE=/absolute/path/target/debug/examples/protocol_stdio npm run test:integration
```

该example只用于受控测试，不能装入生产应用或对真实数据使用。真实桌面验收应先用新建测试草稿完成批准→读取→修改→UI刷新→并发冲突→撤销；付费生成需另行明确授权。

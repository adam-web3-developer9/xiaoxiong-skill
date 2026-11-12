# 验证记录

日期：2026-11-12。所有验证使用临时目录和明确的受控 Host，没有真实生成、购买、生产数据写入、推送或部署。

## 已通过

1. Rust 原生 Agent crate：31/31 tests，通过真正的 lib/server/v1 routes/v2 auth/v2 routes 和真实 config-domain/task-domain/domain 依赖源码。包括 48 个现有 task action 的路由、scope/草稿归属、账户布尔投影、渠道 write-only 凭据、错误不泄密、配对/拒绝/过期/撤销/账户变更/重启隔离、独立版本客户端和默认 v1 禁用。日志 `native-protocol-tests.log`。
另有文件级发现测试验证自选数据根、两份公开描述一致、无token、Unix0600及仅本实例清理。此测试已实际通过，不需socket。

2. Rust BuildIdentity：3/3 tests。保留原有 build gate golden 与 read-only contract，新增 schema 客户端不冒充 renderer 构建、宿主未就绪拒写、sticky mismatch 不被解除。日志 `native-build-gate-tests.log`。
3. 标准 patch `git apply --check` 通过；精确 SHA apply 脚本在一次性副本上通过 check、apply、重复执行零变更和拒绝覆盖其他本地改动。
4. baseline 文件逐个核对 pinned Git tree 的 blob SHA。只删除下载传输额外增加的一个末尾换行，并且仅在删除后 blob SHA 精确匹配上游时处理。

## 环境阻塞 / 未完成

- `actual_unix_socket_serves_v2_and_publishes_no_credentials` 已编译，执行时监听创建返回 OS EPERM（当前沙箱禁用本地 socket bind）。原失败为 1 failed，其余测试通过；后续通过的 31-test 命令显式 `--skip actual_unix_socket`，不是声称该测试通过。测试源码保留，可在允许 socket 的环境重跑。
- Windows named pipe、完整 Tauri app compile/启动、实际 WebView 批准/拒绝/撤销/账户切换尚未验收。
- 旧 golden 集成测试已编译，并显式启用 legacy Session；运行因 pinned 基线的 routes-golden.cjs 仍引用已删除 electron/agent-data-routes.cjs 而失败（MODULE_NOT_FOUND）。这是基线已有 oracle 遗留，未伪造替代 oracle。native/adapter 的 legacy fixture 也已显式 opt-in，但未运行其完整依赖图。
- 专用 native 检查目录移除了原 Agent Cargo manifest 中不相关大型 integration adapter 的 dev-dependencies，以编译这次模块级测试；产品 Cargo manifest 没有修改，产品和领域模块没有 stub。
- 完整 app 内渠道配置中心/桌面 API 的验证由相应实现记录另列；上述受控 Host 路由单测不代表真实持久化已通过。
- 没有真实渠道调用或付费生成；返回结果/任务状态均不能冒称供应商验收。

## 复跑

在完整 AIStudio checkout 应用补丁后运行 README 的原生/前端命令。在本次临时检查目录运行模块测试时使用实际 Rust 工具链；不依赖 Node 或浏览器模拟 Rust 权限逻辑。

后续增改源码后，重新运行相关测试并执行 `python integration/aistudio/scripts/build-patch.py` 刷新 patch/manifest，然后再次 `git apply --check`。

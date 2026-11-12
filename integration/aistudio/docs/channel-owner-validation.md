# 渠道配置 owner 与动态插件读取验收

基准：AIStudio main `5423be21112b61b0728cf10a0bc9883e0e308581`。本次仅写独立项目的集成 overlay，未推送、未打开真实配置库、未调用真实供应商。

## 已实现

- `local-data:channel-config-list`：在原 config owner 查询设置和凭据存在状态，复用 `secret_presence_view`，只返回 `{ providers, version }`。按当前已安装插件 `configSchema` 投影公开字段，保留能力分项与配置状态，去除秘密值、内部引用、指纹、密钥行及携带认证信息的 baseUrl。宿主已有字段不会覆盖 schema 的递归 secret/writeOnly 投影。
- `local-data:channel-config-patch`：原 RuntimeStore 事务中读取最新设置、只合并目标 provider 的所给字段、保存凭据和设置、推进设置版本。支持新增、嵌套 capability 配置、独立 credentials/capabilityCredentials 输入和可选 expectedVersion。未提供凭据保持原值，null 明确清除。标准密钥复用 `collect_secret_writes` / `strip_secret_inputs`；插件其他 secret/writeOnly 字段沿应用现有本地配置存储，不新增第二个凭据所有者。
- 改变 baseUrl、插件绑定或未知插件路由字段时，不能把旧秘密继续送往新目的地。基于宿主真实 `channel_capability_config` 比较所有能力的有效配置，覆盖新增能力、继承凭据、插件自定义秘密和 legacyField。要求同批显式替换或清除仍会携带的旧秘密。能力配置的自定义秘密清除保留空覆盖值，避免删除字段后重新继承根秘密；写入后按真实有效配置检查所给凭据确实生效。名称、启用状态、已知模型/质量/尺寸/并发等非目的地字段正常更新并保留凭据。未知插件字段未声明网络语义，保守按可能改变目的地处理，不按插件 ID 写分支。
- `local-data:style-references-patch`：在同一 owner 只更新已有全局风格的 refImages，不把掩空设置快照回写，不碰秘密或其他设置。
- `local-data:channel-config-options`：只接受已保存 providerId、capability、已声明 action。owner 在同一事务读取 endpoint 与凭据；交给插件的 config 仅含当前 schema 声明字段及 legacyField，排除其他能力配置和内部 secretRef。宿主解析当前插件 binding、校验版本/摘要，复用 `configurationOptions`。保留 UI 协议的 options、columns、description、parameters、selection、status 及其详情字段，不硬编码插件或模型。输出按 schema 秘密字段/legacyField 和实际使用的秘密字面量、Base64、URL 编码脱敏；失败只返回稳定 code 和固定说明。
- 新 IPC 同时登记 sole contract、编译期 channels.json、只读快照和覆盖报告。8 个被修改的既有文件 baseline 均已按 Git blob SHA 核验到固定提交。

## 实际通过

1. 完整 `aistudio-config-store` crate 及真实依赖编译成功；16 个新增 Rust owner 测试通过。覆盖秘密只写读取、任意 schema 私密字段/数组、legacyField、SQLite 实际失败回滚、双连接并发叶更新、版本冲突、继承/新增能力目的地、风格引用、内部动态上下文和 schema 投影。
2. 直接引用生产 `aistudio-desktop-api/src/agent_channels/public.rs` 的独立 Rust 验证 crate 编译成功，2 个动态输出脱敏测试通过。实际上游 desktop-api Cargo.toml 已有 base64 0.22，无需添加依赖。
3. 运行真实 IPC registry，213 个 channels.json 条目与注册表顺序一致；91 条只读快照与 READ_ONLY_CHANNELS 整集一致。
4. 新 Rust 文件通过 rustfmt；baseline SHA 全部吻合固定提交。

复现 owner 定向测试：在应用合入 overlay 后运行 `cargo test --manifest-path crates/aistudio-config-store/Cargo.toml agent_channels`。动态投影测试已内嵌生产文件，会随 desktop-api crate 的正常测试执行。

## 未验收与边界

- 未完整构建 aistudio-desktop-api / Tauri 全应用，未做 Windows named pipe、实际桌面点击、真实插件进程或真实供应商调用；owner 与纯输出投影测试不能替代这些验收。
- 测试使用临时 SQLite 和测试凭据；没有付费生成、登录、真实设置修改、提交或推送。
- 插件仍是用户已信任安装的软件，输出脱敏不是针对恶意插件任意编码或外部网络行为的隔离沙箱。同 OS 用户拥有原配置文件权限时，本地 Agent API 的只写边界不能替代文件系统权限。

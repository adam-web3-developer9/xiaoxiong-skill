# AIStudio 应用端 v2 接入补丁

基线：`adam-web3-developer9/aistudio`，提交 `5423be21112b61b0728cf10a0bc9883e0e308581`（v3.1.74）。这是应用源码补丁，不是另一个业务宿主；未部署或修改用户业务数据。

## 应用

```sh
python integration/aistudio/scripts/apply.py /path/to/aistudio
python integration/aistudio/scripts/apply.py /path/to/aistudio --apply
```

默认只校验。所有旧文件必须精确匹配基线或已经应用的结果；存在其他编辑时拒绝覆盖，先人工合并。完整基线在 `baseline/`，修改后源码在 `overlay/`，标准补丁为 `aistudio-v2.patch`，SHA256 清单为 `manifest.json`。也可先用 `git apply --check aistudio-v2.patch`。应用脚本不会构建、启动、推送、打包或部署。

## 原生边界

- 同一个已有 Unix socket / Windows named pipe 服务运行 v2，复用唯一 DesktopService、账户、构建/更新写闸、事件广播与业务存储。
- 新公开发现文件 `<data-root>/agent-v2-connection.json` 只含协议、schema、endpoint、PID、dataRoot、随机 instanceId、旧版开发模式标志，不含 token。
- 应用也在标准系统临时目录发布 `aistudio-agent-v2-{pid}-{instanceId}.json` 无凭据发现标记，内容与草稿根描述文件完全一致。Unix 文件权限 0600；正常关闭只删除匹配本实例身份的标记。这样无需扫描用户磁盘即可发现便携版或自选数据目录。客户端必须核对标记和根描述文件的一致性及本机所有者；多实例由用户明确选择。
- 默认禁用 v1 握手，也不写出 v1 bearer-token 描述文件。只有启动应用时显式设置 `AISTUDIO_AGENT_LEGACY_V1=1` 才保留旧开发 CLI；这属于不受 v2 授权限制的可信本机开发模式，授权面板显示警告。独立客户端不得读取旧描述文件或自动降级。相同 OS 账户下能任意读应用文件的恶意程序不属于进程隔离保证。
- v2 通过稳定 schema 1 协商，不伪造客户端为应用 renderer 构建。主进程/宿主构建一致、sticky mismatch、账户和退出写栅均保留。
- 批准/拒绝/撤销/列表只由原生 `agent_authorization` Tauri command 提供，必须来自已有 `trusted_window` 的主 WebView，并通过主窗口构建和账户写闸。socket 没有 approve 方法。
- 授权保存在本次应用进程内，绑定账户内部 epoch、身份和应用实例；账户变更或应用重启失效。配对有效期 5 分钟，grant 有效期 8 小时。撤销影响后续请求，不取消已经接受执行的业务命令。
- read/edit/generate/export 与 channels.read/channels.write/channels.credentials.write 分别授权；draftIds `[]` 表示没有草稿，`["*"]` 表示全部。任务权限根据真实任务归属判定，不信任调用者的草稿提示。
- 渠道凭据允许独立授权后写入，永不返回；非密钥配置由已有配置中心原子合并。错误、日志、任务公开视图和渠道返回值不得泄露私钥、Cookie、token 或内部配置快照。

## JSONL 协议

请求 `{id,method,params,token?}`，响应 `{id,ok:true,result}` 或 `{id,ok:false,error:{code,message,details}}`。

1. `hello {protocol:2,schemaVersion:1,client:{id,name,version}}`，无需 token。返回协商结果及实际 capability catalog。
2. `auth.request {scopes,draftIds}` → `{requestId,pollSecret,userCode,expiresAt,intervalMs}`。
3. 用户在应用内确认客户端显示名称、scope 和草稿范围；名称是客户端自报身份。userCode 只供核对，不能取代高熵 pollSecret。
4. `auth.poll {requestId,pollSecret}` → 待批准 `{status:"pending",intervalMs}`；批准后一次性返回 `{sessionId,token,scopes,draftIds,expiresAt}`。
5. `auth.resume {sessionId,token}` 在当前或新建并完成 hello 的连接上绑定 grant。后续业务请求在 envelope 中携带 token。
6. `auth.logout {}`（envelope token）撤销当前 grant。

expiresAt 是 UTC 毫秒时间戳。配对或领取凭证断线后不自动重试写入。poll 领取凭证仅成功一次，回执丢失时重新申请授权。客户端只在本机受限凭据存储保存 token，不把 token/pollSecret 写到 stdout 或对话。

UI 调用：`invoke('agent_authorization',{action:'list'|'approve'|'deny'|'revoke',requestId?,sessionId?})`。返回 sanitized pending/grants 和 legacyTrustedDevelopment，绝不含 pollSecret 或 token。

## 验证与范围

- 原生 bridge/lib/server/v1 routes/v2 auth/v2 routes 已用真实 domain/config-domain/task-domain 源码及生成合同构建并运行；为隔离已存在大型 adapter 测试依赖，验证目录只移除了原 Cargo manifest 的无关 dev-dependencies，没有替换产品模块或领域实现。具体计数看 `VALIDATION.md`。
- BuildIdentity 完整真实模块与已有 golden、read-only 合同及新增 agent schema 回归单独编译通过。
- 新增真实 Unix socket 测试已编译，当前沙箱禁止 bind，执行返回 OS EPERM；不能称为 socket 或应用端到端通过。Windows named pipe 未运行。
- 主 Tauri 应用完整编译、真实窗口批准/撤销、账户切换、Windows/macOS 安装版和实际渠道请求仍需正式环境验收。测试中的 Host 是显式控制夹具，不代表真实业务生成。
- 没有运行真实生成、购买、生产数据修改、推送或部署。

在完整 AIStudio checkout 中的建议检查：

```sh
cargo test --manifest-path crates/aistudio-agent-data/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml build_identity
cargo check --manifest-path src-tauri/Cargo.toml
pnpm typecheck
```

旧 v1 adapter 测试需显式开启开发兼容环境变量；普通运行保持默认关闭。

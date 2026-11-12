'use strict';
// IPC 合同注册表（docs/architecture-design.md 3.7 / G10）：app 里每一条 ipcMain.handle 频道都在这里登记，
// 写明由谁服务（main = 主进程自己处理；host = AI 任务宿主进程，主进程只转发；native = Rust 桌面 API 直接实现，
// 没有 electron 时代的主进程 handler）。
//
// 宿主频道只认 ai-task-host-protocol.cjs 的 AI_TASK_HOST_CHANNELS，这里直接引用不抄一份——
// 两份清单一漂移，主进程就不会给新频道注册转发器，渲染层调用会报「没有 handler」。
// 新增主进程频道：在 MAIN_CHANNELS 对应模块下加一行；新增宿主频道：加进 ai-task-host-protocol.cjs。
// electron/tests/ipc-contract.test.cjs 会核对 preload 调用、各文件的 handle 注册和这份表三方一致。
// 请求 / 响应类型、生成 preload 与 d.ts 是后续步骤，这一版先把「有哪些频道、谁服务」收成一处。

const { AI_TASK_HOST_CHANNELS } = require('./ai-task-host-protocol.cjs');

// 主进程自己服务的频道，按注册它的文件分组（排查时按 module 找实现）。
const MAIN_CHANNELS = Object.freeze({
  'account/ipc.cjs': ['account:status', 'account:action'],
  'main.cjs': [
    'open-external-url',
    'open-tutorial-window',
    'app-update-check',
    'app-update-download',
    'app-update-cancel',
    'app-update-channel',
    'app-update-install',
    'app-update-get-status',
    // 宿主当前纪元（docs/task-system-implementation.md 5 P4）：页面读基线用，补 host:ready 广播接不住的场景。
    'host:current-epoch',
    'host:build-info',
    'host:verify-build'
  ],
  'local-data.cjs': [
    'local-data:get-info',
    'local-data:kv-get',
    'local-data:settings-snapshot',
    'local-data:drafts-list',
    'local-data:drafts-list-summaries',
    'local-data:drafts-get',
    'local-data:image-editor-list',
    'local-data:image-editor-get',
    'local-data:rows-get',
    'local-data:revisions',
    'local-data:draft-summary-snapshot',
    // 密钥（C9-K）：渲染层只有写 / 删 / 问状态三个口（secret-get 明文通道已删）
    'local-data:secret-set',
    'local-data:secret-delete',
    'local-data:secret-status',
    'local-data:secret-store-state',
    // 任务表只有宿主写（状态走 ai-task:* 命令），主进程只留只读列表；tasks-upsert/delete/clear 已删（G4）
    'local-data:tasks-list',
    'local-data:execution-logs-list'
  ],
  'task-history-ipc.cjs': [
    'task-history:query',
    'task-history:stats',
    'task-history:detail',
    'task-history:facets',
    'task-history:info',
    'task-history:latest-terminal',
    'task-history:export',
    'task-history:reveal'
  ],
  'network-proxy.cjs': [
    'network-proxy:get-status',
    'network-proxy:apply',
    'network-proxy:test'
  ],
  // 抖音：页面预览解析（只读）与「选保存位置」对话框留主进程；下载本体是宿主任务（local-job:start kind=douyinDownload）。
  'douyin-download.cjs': [
    'douyin:resolve',
    'douyin:choose-destination'
  ],
  'zhihu-manuscript.cjs': [
    'zhihu:read-manuscript',
    'zhihu:resolve-link'
  ],
  'prompt-export.cjs': [
    'prompts:export-to-folder'
  ],
  'folder-rename.cjs': [
    'folder-rename:select',
    'folder-rename:list',
    'folder-rename:apply',
    'folder-rename:restore'
  ],
  'chatgpt-relay.cjs': [
    'chatgpt-relay:status',
    'chatgpt-relay:login',
    'chatgpt-relay:check-login',
    'chatgpt-relay:clear-login',
    'chatgpt-relay:clear-skipped',
    'chatgpt-relay:list-skipped'
  ]
});

// 只发不等的频道（ipcRenderer.send / ipcMain.on，没有回执）：埋点、诊断及退出冲刷——渲染层不能等诊断的回包，
// 也不能因为它失败而抛错。和上面 invoke/handle 的频道分开核对（ipc-contract.test.cjs 的第 8 步）。
const SEND_CHANNELS = Object.freeze({
  'main.cjs': ['telemetry:track', 'telemetry:renderer-ready', 'diagnostics:renderer', 'app:flush-ready', 'app:flush-done']
});

// 原生频道：由 Rust 桌面 API 直接实现，没有 electron 时代的主进程 handler——插件中心与「显式揭示本地配置」
// 是在原生后台迁移之后新增的，当时只把生成物 channels.json 手工补了 11 条，契约源没有登记，导致
// preload 调用 ⊆ 注册表 这条核对长期是红的、channels.json 也与契约漂移。现在以契约为唯一源：
// module 记 Rust 实现文件，servedBy 记 'native'，test:ipc-contract 直接核对里面的 supports 路由清单。
const NATIVE_CHANNELS = Object.freeze({
  'src-tauri/src/bridge.rs': ['diagnostics:export'],
  'crates/aistudio-desktop-api/src/tasks.rs': ['ai-task:detail'],
  'crates/aistudio-desktop-api/src/channel_plugins.rs': [
    'channel-plugins:seed_status',
    'channel-plugins:initialize_seeds',
    'channel-plugins:catalog',
    'channel-plugins:list',
    'channel-plugins:configuration_options',
    'channel-plugins:choose_archive',
    'channel-plugins:install',
    'channel-plugins:install_catalog',
    'channel-plugins:set_enabled',
    'channel-plugins:remove',
    'channel-plugins:get_config',
    'channel-plugins:save_config'
  ],
  'crates/aistudio-desktop-api/src/data.rs': [
    'local-data:secret-reveal',
    'local-data:secret-import-legacy',
    'local-data:channel-config-list',
    'local-data:channel-config-patch',
    'local-data:channel-config-options',
    'local-data:style-references-patch'
  ],
  'crates/aistudio-desktop-api/src/joojo_login.rs': [
    'joojo:login-open',
    'joojo:login-capture',
    'joojo:login-close'
  ],
  'crates/aistudio-desktop-api/src/jianying.rs': [
    'jianying:select-draft-dir'
  ]
});

// 主进程广播、不等回执的频道（webContents.send / broadcastToWindows，preload 用 ipcRenderer.on 暴露）：
// 方向和 SEND_CHANNELS 相反（主进程 -> 渲染层）。目前只登记 resync-on-gap 这次新增的 power:resync
// （docs/architecture-design.md 8.2 第 3a 条）。'host:ready' / 'local-data:changed' 是这次改动之前就有的
// 广播，不在这次迁移范围内，先记在 electron/tests/ipc-contract.test.cjs 的 BROADCAST_KNOWN_UNREGISTERED。
const BROADCAST_CHANNELS = Object.freeze({
  'account/service.cjs': ['account:changed'],
  'main.cjs': ['power:resync', 'app-update-status', 'host:build-mismatch', 'app:shutdown-notice'],
  'renderer-flush.cjs': ['app:flush-request']
});

// 宿主 → 主进程的 rpc 能力清单（electron/main-capabilities.cjs 的 MAIN_CAPABILITIES 是它们的实现，
// 同一份名单；test:ipc-contract 核对宿主里每个 rpc / callMainCapability 调用点都在表里、表里每项都有 handler，V4）。
const RPC_CAPABILITIES = Object.freeze([
  'native:media-commit', 'native:media-claim', 'native:media-complete', 'native:media-save', 'native:media-export', 'native:process', 'native:provider', 'native:export', 'native:history-export',
  'secrets:decode-rows', 'secrets:prepare-writes', 'secrets:notify-committed',
  'archive:flush-renderers',
  'network-proxy:resolve-system',
  'account:refresh',
  'account:cloud-request',
  'account:cloud-cancel',
  'dialog:showOpenDialog',
  'dialog:showSaveDialog',
  'dialog:showMessageBox',
  'douyin:resolve-page'
]);

const READ_ONLY_CHANNELS = new Set([
  'local-data:channel-config-list', 'local-data:channel-config-options',
  'diagnostics:export', 'ai-task:detail',
  'channel-plugins:seed_status',
  'channel-plugins:configuration_options',
  'typetale:list', 'typetale:runtime', 'typetale:params', 'typetale:ui', 'typetale:choose-archive', 'typetale:choose-python',
  'package-sharing:progress', 'package-sharing:cancel',
  'local-data:settings-transfer-export', 'local-data:settings-transfer-preview', 'local-data:settings-transfer-cancel',
  'account:status', 'account:action',
  'host:current-epoch', 'host:build-info', 'host:verify-build',
  'prompt:snapshot', 'prompt:revision', 'prompt:preview', 'local-data:rows-get', 'local-data:revisions', 'local-data:draft-summary-snapshot',
  'open-external-url', 'open-tutorial-window', 'app-update-check', 'app-update-get-status', 'app-update-install', 'app-update-download', 'app-update-cancel', 'app-update-channel',
  'local-data:get-info', 'local-data:kv-get', 'local-data:settings-snapshot', 'local-data:drafts-list', 'local-data:drafts-list-summaries',
  'local-data:drafts-get', 'local-data:image-editor-list', 'local-data:image-editor-get', 'local-data:secret-status', 'local-data:secret-store-state',
  'local-data:tasks-list', 'local-data:execution-logs-list', 'local-data:video-durations',
  'task-history:query', 'task-history:stats', 'task-history:detail', 'task-history:facets', 'task-history:info',
  'task-history:latest-terminal', 'task-history:export', 'task-history:reveal', 'network-proxy:get-status',
  'video-analysis:select', 'video-analysis:inspect',
  'video-analysis:list', 'video-analysis:list-summaries', 'video-analysis:get', 'video-analysis:export-report',
  'folder-rename:select', 'folder-rename:list', 'chatgpt-relay:status', 'chatgpt-relay:check-login', 'chatgpt-relay:list-skipped',
  'ai-task:manual-relay-list', 'ai-task:manual-relay-get', 'batch:get', 'batch:list-active', 'batch:ledger', 'batch:progress',
  'workflow:list', 'workflow:get', 'public-media:status', 'telemetry:report', 'search:query',
  'env:gate-status', 'ai-task:estimate', 'batch:estimate', 'workflow:estimate', 'local-data:project-export',
  'local-data:project-preview', 'local-data:project-progress', 'local-data:project-backups', 'local-data:project-cancel',
  'local-data:library-transfer-export', 'local-data:library-transfer-preview'
]);

function buildContract() {
  const entries = [];
  const seen = new Map();
  const add = (channel, servedBy, module) => {
    if (seen.has(channel)) {
      // 同一频道登记两次 = 两边都注册 handler，Electron 会在第二次 handle 时直接抛错；在这里就拦下
      throw new Error(`IPC 频道重复登记：${channel}（${seen.get(channel)} 与 ${servedBy}）`);
    }
    seen.set(channel, servedBy);
    entries.push(Object.freeze({ channel, servedBy, ...(module ? { module } : {}) }));
  };
  Object.entries(MAIN_CHANNELS).forEach(([module, channels]) => channels.forEach((channel) => add(channel, 'main', module)));
  AI_TASK_HOST_CHANNELS.forEach((channel) => add(channel, 'host'));
  Object.entries(NATIVE_CHANNELS).forEach(([module, channels]) => channels.forEach((channel) => add(channel, 'native', module)));
  return Object.freeze(entries);
}

const IPC_CONTRACT = buildContract();
const CONTRACT_BY_CHANNEL = new Map(IPC_CONTRACT.map((entry) => [entry.channel, entry]));

function contractEntry(channel) {
  return CONTRACT_BY_CHANNEL.get(String(channel)) || null;
}

/** @param {'main' | 'host' | 'native'} servedBy */
function channelsServedBy(servedBy) {
  return IPC_CONTRACT.filter((entry) => entry.servedBy === servedBy).map((entry) => entry.channel);
}

module.exports = { IPC_CONTRACT, MAIN_CHANNELS, NATIVE_CHANNELS, SEND_CHANNELS, BROADCAST_CHANNELS, READ_ONLY_CHANNELS, RPC_CAPABILITIES, contractEntry, channelsServedBy };


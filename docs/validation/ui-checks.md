# 授权面板定向验证

工作目录：`/tmp/aistudio-ui-validation`。所有 npm 依赖仅在这个临时目录，未加入项目依赖或交付源码。

源基线：`adam-web3-developer9/aistudio@5423be21112b61b0728cf10a0bc9883e0e308581`。
实际 AppDrawer 来源：[src/ui/AppDrawer.tsx](https://github.com/adam-web3-developer9/aistudio/blob/5423be21112b61b0728cf10a0bc9883e0e308581/src/ui/AppDrawer.tsx)；以归档正文 SHA256 为准。

固定提交下载的完整未修改正文在 `ui-baseline-AppDrawer.tsx.txt`，验证时的真实路径 `/tmp/aistudio-ui-validation/src/ui/AppDrawer.tsx`。不是虚构 AppDrawer，也没有用 div 替换 Drawer。DOM 检查只 mock 原生 IPC 和浮层协调器；浮层键盘/导航仲裁不在这次隔离验收范围内。其余 React/antd/AppDrawer 使用真实实现。

## 已存在临时目录：复跑最终 overlay

从项目根 `/workspace/shared/xiaoxiong-agent` 执行：

```sh
cp -r integration/aistudio/overlay/src/core /tmp/aistudio-ui-validation/src/
cp -r integration/aistudio/overlay/src/store /tmp/aistudio-ui-validation/src/
cp -r integration/aistudio/overlay/src/features /tmp/aistudio-ui-validation/src/
cp docs/validation/ui-baseline-AppDrawer.tsx.txt /tmp/aistudio-ui-validation/src/ui/AppDrawer.tsx
cp docs/validation/ui-dom-legacy-check.mjs.txt /tmp/aistudio-ui-validation/src/features/agent-authorization/authorization.dom-check.mjs
cd /tmp/aistudio-ui-validation
./node_modules/.bin/tsc --noEmit
node src/features/agent-authorization/authorization.dom-check.mjs
```

## 新临时目录：重建同一验收环境

Node 24。项目基线已使用 React 18、antd 5.23、TypeScript 和 jsdom；此隔离工作目录额外锁定安装版本，不改变项目 package.json。原始锁文件保存在 `ui-validation-package-lock.json`。

```sh
mkdir -p /tmp/aistudio-ui-validation/src/ui/overlays
npm install --prefix /tmp/aistudio-ui-validation --cache /tmp/aistudio-ui-npm-cache --ignore-scripts --no-audit --no-fund typescript@5.7.3 react@18.3.1 react-dom@18.3.1 @types/react@18.3.18 @types/react-dom@18.3.5 antd@5.23.0 @tauri-apps/api@2.10.1 jsdom@26.1.0
cat > /tmp/aistudio-ui-validation/tsconfig.json <<'JSON'
{"compilerOptions":{"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","jsx":"react-jsx","strict":true,"skipLibCheck":true,"noEmit":true},"include":["src/**/*.ts","src/**/*.tsx"]}
JSON
cat > /tmp/aistudio-ui-validation/src/ui/overlays/OverlayProvider.ts <<'TS'
export function useOverlay(_options: { id: string; layer: string; parent?: string; open: boolean; close: () => void }): void {}
TS
cat > /tmp/aistudio-ui-validation/src/ui/overlays/overlayLogic.d.ts <<'TS'
export type OverlayLayer = 'panel' | 'modal';
TS
```

然后执行上一节复制与验证命令。`tsx` CLI 的早期尝试因沙箱 IPC pipe `EPERM` 未启动测试；最终持久化 DOM 检查使用 Node 原生 registerHooks + TypeScript 转译，不依赖 tsx，也不需要监听网络/Unix socket。

## 覆盖与边界

- DOM：新 pending 自动展示、确认码与私钥仅写文案、关闭/重新打开、双击只发一次 approve、授权回执展示、revoke 精确 sessionId、过期请求不能批准；legacyTrustedDevelopment 缺失/false 不显示警告，显式 true 展示旧协议不受授权、草稿范围和撤销控制的醒目警告及顶栏提示。
- TypeScript：新增模型、store、feature 与真实 AppDrawer，严格类型；现有浮层协调器用类型契约隔离。没有对全量 App.tsx/SettingsPage 及其数百依赖做全项目类型检查。
- 三个纯函数/源码挂载测试已复跑，见 ui-unit-tests.txt。
- 尚未验收：完整桌面构建、原生 Tauri 实际登录/授权、真实账号/渠道配置、编辑器脏缓冲与浮层仲裁的端到端运行。没有调用真实生成、没有修改生产数据。

最终 DOM harness 是原 DOM 检查加 legacy true/false 断言，完整归档在 ui-dom-legacy-check.mjs.txt。最终源码与 harness 的 SHA256 见 ui-verified-sha256.txt。

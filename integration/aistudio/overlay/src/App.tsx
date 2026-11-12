// minor（2026-10-01 第 2 轮审查修复）：navGroups.tsx 抽取侧栏图标定义之后，这 13 个图标在 App.tsx
// 里已经没有别的用处（buildMenuItems 现在只读 navGroups 里带来的 icon 字段），移除未使用的 import。
import { ArrowLeftOutlined, MenuFoldOutlined, MenuUnfoldOutlined, ReloadOutlined, SearchOutlined, SettingOutlined, QuestionCircleOutlined } from '@ant-design/icons';
// important（2026-10-01 第 2 轮审查打回，修复第 1 轮遗留）：原来两处 `Modal.info(...)`（批量重查失败
// 清单、服务端原始响应）已经改成 state 驱动的 `AppModal`（见 GlobalTaskCenter 文末两处渲染），
// App.tsx 不再直接 import antd 的 `Modal`，改回 scripts/renderer-layers-baseline.json 的基线名单。
import { Alert, Button, ConfigProvider, Layout, Menu, Spin, Tooltip, Typography, message } from 'antd';
import { productTheme } from './ui/productTheme';
import type { MenuProps } from 'antd';
import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';
import { EnvTakeoverBanner } from './features/env-takeover/EnvTakeoverBanner';
import { FailedDraftWrites } from './features/projects/FailedDraftWrites';
import { BuildCompatibilityBanner } from './features/runtime/BuildCompatibilityBanner';
import { RouteTracker } from './app/telemetry/RouteTracker';
import { StartupPageReady } from './app/telemetry/StartupPageReady';
import { RouteErrorBoundary } from './app/RouteErrorBoundary';
import { DraftsPage } from './pages/DraftsPage';
import { loadDraftEditorPage } from './pages/draftEditorRoute';
import { NavigatorProvider } from './app/navigation/navigator';
import { useNavigator } from './store/navigation/navigator';
import { ContextBreadcrumbs } from './app/navigation/ContextBreadcrumbs';
import { NavigationBreadcrumbProvider } from './store/navigation/breadcrumbs';
import { currentPageOf } from './store/navigation/navigatePlan';
import { navGroups } from './app/navigation/navGroups';
import { openSearchPalette, registerSearchPalette, type PaletteOpenOptions } from './store/search/palettePort';
import { registerShortcut } from './ui/shortcuts/registry';
import { ShortcutsHelpPanel } from './app/shortcuts/ShortcutsHelpPanel';
import { TaskSubmissionNotice } from './features/orchestration/TaskSubmissionNotice';
import appIconUrl from './assets/app-icon.png';
import { subscribeDownloadDebug } from './store/downloadDebugClient';
import { useAppSettings } from './store/settingsStore';
import { failStartupCover } from './store/startupCover';
import { AgentAuthorizationPanel } from './features/agent-authorization/AgentAuthorizationPanel';
import { AccountMenu } from './features/account/AccountPanel';

const { Header, Sider, Content } = Layout;
const { Text } = Typography;
const SearchPalette = lazy(() => import('./features/search/SearchPalette').then((module) => ({ default: module.SearchPalette })));
const GlobalTaskCenter = lazy(() => import('./features/task-center/GlobalTaskCenter').then((module) => ({ default: module.GlobalTaskCenter })));
const ManualRelayDrawer = lazy(() => import('./features/assistant/ManualRelayDrawer').then((module) => ({ default: module.ManualRelayDrawer })));

function ActivatedSearchPalette({ options, onReady }: { options: PaletteOpenOptions; onReady: () => void }) {
  useLayoutEffect(onReady, [onReady]);
  useEffect(() => {
    openSearchPalette(options);
  }, [options]);
  return <SearchPalette />;
}

function DeferredSearchPalette() {
  const [options, setOptions] = useState<PaletteOpenOptions | null>(null);
  const [ready, setReady] = useState(false);
  const cleanup = useRef<(() => void) | null>(null);
  const onReady = useCallback(() => { cleanup.current?.(); setReady(true); }, []);
  useEffect(() => {
    if (ready) return;
    const unregisterPalette = registerSearchPalette(setOptions);
    const activate = (event: KeyboardEvent) => {
      event.preventDefault();
      const selected = (window.getSelection?.()?.toString() || '').trim();
      setOptions({ query: selected || undefined });
    };
    const unregisterK = registerShortcut({ keys: 'mod+k', scope: 'global', label: '打开搜索', action: activate });
    const unregisterShiftF = registerShortcut({ keys: 'mod+shift+f', scope: 'global', label: '打开搜索（带选区）', action: activate });
    cleanup.current = () => { unregisterPalette(); unregisterK(); unregisterShiftF(); };
    return cleanup.current;
  }, [ready]);
  if (!options) return null;
  return <RouteErrorBoundary><Suspense fallback={<div role="status">正在加载搜索...</div>}>
    <ActivatedSearchPalette options={options} onReady={onReady} />
  </Suspense></RouteErrorBoundary>;
}

function DeferredManualRelayDrawer(props: { open: boolean; onClose: () => void }) {
  const [activated, setActivated] = useState(false);
  useEffect(() => { if (props.open) setActivated(true); }, [props.open]);
  if (!props.open && !activated) return null;
  return <RouteErrorBoundary><Suspense fallback={<div role="status">正在加载人工中继...</div>}>
    <ManualRelayDrawer {...props} />
  </Suspense></RouteErrorBoundary>;
}
// The main project list loads with the shell; secondary pages remain on demand.
const CanvasPage = lazy(() => import('./pages/CanvasPage').then((module) => ({ default: module.CanvasPage })));
const DraftEditorPage = lazy(() => loadDraftEditorPage().then((module) => ({ default: module.DraftEditorPage })));
const DirectorKnowledgePage = lazy(() => import('./pages/DirectorKnowledgePage').then((module) => ({ default: module.DirectorKnowledgePage })));
const SeriesStructurePage = lazy(() => import('./pages/SeriesStructurePage').then((module) => ({ default: module.SeriesStructurePage })));
const SettingsPage = lazy(() => import('./pages/SettingsPage').then((module) => ({ default: module.SettingsPage })));
const TaskCenterPage = lazy(() => import('./pages/TaskCenterPage').then((module) => ({ default: module.TaskCenterPage })));
const StudioSkillsPage = lazy(() => import('./pages/StudioSkillsPage').then((module) => ({ default: module.StudioSkillsPage })));
const VideoToolboxPage = lazy(() => import('./pages/VideoToolboxPage').then((module) => ({ default: module.VideoToolboxPage })));
const ToolboxPage = lazy(() => import('./pages/ToolboxPage').then((module) => ({ default: module.ToolboxPage })));
const UsageStatsPage = lazy(() => import('./features/usage-stats/UsageStatsPage').then((module) => ({ default: module.UsageStatsPage })));
const PluginCenterPage = lazy(() => import('./pages/PluginCenterPage').then((module) => ({ default: module.PluginCenterPage })));
const ImageEditorPage = lazy(() => import('./pages/ImageEditorPage').then((module) => ({ default: module.ImageEditorPage })));

function RouteLoading() {
  return (
    <div className="environment-loading" role="status" aria-live="polite">
      <Spin size="large" />
      <Typography.Text type="secondary">正在加载页面...</Typography.Text>
    </div>
  );
}

// 菜单和搜索从同一份本地页面目录派生。
function buildMenuItems(): MenuProps['items'] {
  return navGroups
    .map((group) => ({
      key: group.key,
      label: group.label,
      type: 'group',
      children: group.children.map(({ key, label, icon, disabled }) => ({
        key,
        label,
        icon,
        ...(disabled ? { disabled: true, title: disabled } : {})
      }))
    }));
}

function selectedKey(pathname: string) {
  if (pathname.startsWith('/draft/')) {
    return '/';
  }
  return pathname;
}


function pushDownloadDebug(payload: Record<string, unknown>) {
  const entry = {
    time: new Date().toISOString(),
    ...payload
  };
  const target = window as typeof window & { __AISTUDIO_DOWNLOAD_DEBUG__?: Record<string, unknown>[] };
  target.__AISTUDIO_DOWNLOAD_DEBUG__ = [...(target.__AISTUDIO_DOWNLOAD_DEBUG__ || []), entry].slice(-200);
  console.log('[download-debug]', entry);
}

// U4 返回栈的顶栏按钮（docs/ui-interaction-implementation.md 4）：只能在 NavigatorProvider 的子树里
// 用 useNavigator()，StudioShell 自己的函数体在 Provider 之外（Provider 是它返回的 JSX 的一部分），
// 所以拆成这个小组件挂在 Header 里，和 Alt+← / 鼠标后退键（navigator.tsx 里绑定）是同一份栈。
function StudioBackButton() {
  const { back, canGoBack } = useNavigator();
  return (
    <Tooltip title="返回上一个位置（Alt+←）">
      <Button
        className="studio-back-button"
        type="text"
        icon={<ArrowLeftOutlined />}
        disabled={!canGoBack}
        onClick={() => void back()}
      />
    </Tooltip>
  );
}

function StudioShellContent() {
  const location = useLocation();
  const { navigate } = useNavigator();
  const menuItems = useMemo(() => buildMenuItems(), []);
  const defaultOpenKeys = ['workspace', 'creation', 'resources', 'system'];
  const [siderCollapsed, setSiderCollapsed] = useState(() => window.localStorage.getItem('aistudio.siderCollapsed') === 'true');
  const [compactViewport, setCompactViewport] = useState(false);

  useEffect(() => {
    window.localStorage.setItem('aistudio.siderCollapsed', String(siderCollapsed));
  }, [siderCollapsed]);

  useEffect(() => {
    const unsubscribe = subscribeDownloadDebug((payload) => {
      pushDownloadDebug({
        event: 'ipc:download-debug',
        payload
      });
    });
    return unsubscribe;
  }, []);

  const navigateFromMenu = useCallback((key: string) => navigate(
    { page: key === '/' ? 'drafts' : key.slice(1) }, { intent: 'jump', source: 'menu' }
  ), [navigate]);
  const page = currentPageOf(location.pathname).page;

  return (
    <>
    <DeferredSearchPalette />
    <ShortcutsHelpPanel />
    <TaskSubmissionNotice />
    <Layout className={`studio-layout product-app page-${page} ${siderCollapsed || compactViewport ? 'studio-layout-sider-collapsed' : ''} ${compactViewport ? 'studio-layout-compact' : ''}`}>
      <RouteTracker />
      <Sider
        className="studio-sider"
        width={216}
        collapsedWidth={64}
        collapsed={siderCollapsed || compactViewport}
        breakpoint="lg"
        onBreakpoint={setCompactViewport}
        trigger={null}
      >
        <div className="brand-block">
          <div className="brand-mark"><img src={appIconUrl} alt="小熊AI" /></div>
          <div className="brand-copy">
            <Text strong>小熊AI</Text>
            <Text type="secondary" className="brand-subtitle">
              创作工作空间
            </Text>
          </div>
          <Button
            className="sider-collapse-button"
            type="text"
            size="small"
            icon={siderCollapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
            aria-label={siderCollapsed ? '展开导航' : '收起导航'}
            onClick={() => setSiderCollapsed((current) => !current)}
          />
        </div>
        <Menu
          className="studio-menu"
          defaultOpenKeys={defaultOpenKeys}
          items={menuItems}
          mode="inline"
          onClick={({ key }) => void navigateFromMenu(String(key))}
          selectedKeys={[selectedKey(location.pathname)]}
        />
        <div className="studio-sidebar-footer">
          <AccountMenu />
          <Tooltip title="快捷键帮助"><Button type="text" icon={<QuestionCircleOutlined />} aria-label="快捷键帮助" onClick={() => window.dispatchEvent(new CustomEvent('aistudio:shortcuts-help'))} /></Tooltip>
        </div>
      </Sider>
      <Layout className="studio-main">
        <Header className="studio-header">
          <div className="studio-header-navigation">
            <StudioBackButton />
            <ContextBreadcrumbs />
          </div>
          <div className="studio-header-actions">
            <AgentAuthorizationPanel />
            <Button className="workspace-search" type="text" icon={<SearchOutlined />} aria-label="全局搜索" onClick={() => openSearchPalette({})}><span>搜索项目与资源</span><kbd>Ctrl K</kbd></Button>
            <RouteErrorBoundary><Suspense fallback={<Button size="small" loading>任务管理</Button>}>
              <GlobalTaskCenter renderManualRelayDrawer={(props) => <DeferredManualRelayDrawer {...props} />} />
            </Suspense></RouteErrorBoundary>
            <Button type="text" icon={<SettingOutlined />} aria-label="打开全局设置" onClick={() => void navigate({ page: 'settings' }, { intent: 'jump', source: 'menu' })} />
          </div>
        </Header>
        {/* important（env-takeover 一轮审查，架构 8.2b 第 5 条）：待接管通知挂在应用外壳顶层，
            不再只出现在任务中心页面——大多数时候人是在草稿编辑器或批量小助手里，任务在那里悄悄
            停跑，不会碰巧点开任务中心才看见这条通知。 */}
        <EnvTakeoverBanner />
        <BuildCompatibilityBanner />
        <FailedDraftWrites />
        <Content className="studio-content">
          <RouteErrorBoundary key={location.pathname}>
          <Suspense fallback={<RouteLoading />}>
          <StartupPageReady>
          <Routes>
            <Route index element={<DraftsPage />} />
            <Route path="draft/:id" element={<DraftEditorPage />} />
            <Route path="series-structure/:id" element={<SeriesStructurePage />} />
            <Route path="canvas" element={<CanvasPage />} />
            <Route path="image-editor" element={<ImageEditorPage />} />
            <Route path="skills" element={<StudioSkillsPage />} />
            <Route path="director-library" element={<DirectorKnowledgePage />} />
            <Route path="video-toolbox" element={<VideoToolboxPage />} />
            <Route path="toolbox" element={<ToolboxPage />} />
            <Route path="douyin-download" element={<Navigate to="/toolbox?tab=douyin" replace />} />
            <Route path="task-center" element={<TaskCenterPage />} />
            <Route path="usage-stats" element={<UsageStatsPage />} />
            <Route path="plugin-center" element={<PluginCenterPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
          </StartupPageReady>
          </Suspense>
          </RouteErrorBoundary>
        </Content>
      </Layout>
    </Layout>
    </>
  );
}

function StudioShell() {
  const settingsStore = useAppSettings();
  useEffect(() => {
    if (settingsStore.status === 'error') failStartupCover();
  }, [settingsStore.status]);
  if (settingsStore.status === 'loading') {
    return <div className="environment-loading"><Spin size="large" /><Typography.Text type="secondary">正在加载环境...</Typography.Text></div>;
  }
  if (settingsStore.status === 'error') {
    return <div className="environment-loading">
      <Alert type="error" showIcon message="读取设置失败，页面和自动流程都已暂停" description={settingsStore.error} />
      <Button type="primary" icon={<ReloadOutlined />} onClick={() => void settingsStore.reload().catch((error: unknown) => console.error('[settings] 重新读取失败', error))}>重试</Button>
    </div>;
  }
  return <NavigatorProvider><NavigationBreadcrumbProvider><StudioShellContent /></NavigationBreadcrumbProvider></NavigatorProvider>;
}

export function App() {
  // 埋点初始化与 React commit 首帧在 bootstrap 里，页面可用在路由 Suspense 里报告。
  // 这里不重复调用。
  return (
    <ConfigProvider theme={productTheme}>
      <Routes>
        <Route path="/*" element={<StudioShell />} />
      </Routes>
    </ConfigProvider>
  );
}


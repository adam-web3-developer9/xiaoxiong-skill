import { lazy, Suspense } from 'react';
import { Alert, Button } from 'antd';
import { SettingsPage as Configuration } from '../features/configuration/SettingsPage';
import { JianyingExportSettingsForm } from '../features/delivery/JianyingExportSettingsForm';
import { TaskHistorySettingsCard } from '../features/task-center/TaskHistorySettingsCard';
import { openAgentAuthorization } from '../store/agentAuthorizationClient';

const PromptCenter = lazy(() => import('../features/prompt-center/PromptCenter').then(module => ({ default: module.PromptCenter })));

export function SettingsPage() {
  return <>
    <Alert type="info" showIcon style={{ marginBottom: 16 }} message="Skill / OpenCode 外部工具连接"
      description="在终端发起登录，在应用中确认一次；可随时查看权限与撤销连接。已有渠道私钥不会开放读取。"
      action={<Button onClick={openAgentAuthorization}>管理授权</Button>} />
    <Configuration
      promptCenter={<Suspense fallback={<p>载入提示词中心…</p>}><PromptCenter /></Suspense>}
      JianyingSettingsControl={JianyingExportSettingsForm}
      taskHistorySettings={<TaskHistorySettingsCard />}
    />
  </>;
}

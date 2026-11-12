import { lazy, Suspense } from 'react';
import { SettingsPage as Configuration } from '../features/configuration/SettingsPage';
import { JianyingExportSettingsForm } from '../features/delivery/JianyingExportSettingsForm';
import { TaskHistorySettingsCard } from '../features/task-center/TaskHistorySettingsCard';

const PromptCenter = lazy(() => import('../features/prompt-center/PromptCenter').then(module => ({ default: module.PromptCenter })));

export function SettingsPage() {
  return <Configuration
    promptCenter={<Suspense fallback={<p>载入提示词中心…</p>}><PromptCenter /></Suspense>}
    JianyingSettingsControl={JianyingExportSettingsForm}
    taskHistorySettings={<TaskHistorySettingsCard />}
  />;
}

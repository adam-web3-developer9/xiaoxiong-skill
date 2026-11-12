import { Alert, Badge, Button, Card, Empty, Space, Tag, Typography } from 'antd';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AppDrawer } from '../../ui/AppDrawer';
import { agentAuthorizationClient, subscribeAgentAuthorizationOpen, type AgentAuthorizationAction } from '../../store/agentAuthorizationClient';
import { agentDraftScopeLabel, agentScopeLabels, canApproveAgentRequest, unseenAgentRequests, type AgentAuthorizationSnapshot } from '../../core/agent-authorization/authorization';

const { Text, Paragraph } = Typography;
const emptySnapshot: AgentAuthorizationSnapshot = { pending: [], grants: [] };

function ScopeSummary({ scopes, draftIds }: { scopes: string[]; draftIds: string[] }) {
  return <>
    <ul style={{ paddingInlineStart: 20 }}>{scopes.map(scope => <li key={scope}>{agentScopeLabels[scope] || scope}</li>)}</ul>
    <Paragraph style={{ overflowWrap: 'anywhere' }}><Text strong>草稿范围：</Text>{agentDraftScopeLabel(draftIds)}</Paragraph>
    {scopes.some(scope => scope.startsWith('channels.')) && <Paragraph type="secondary">渠道权限作用于全局渠道配置，不受草稿范围限制；已有私钥永不开放读取。</Paragraph>}
  </>;
}

/** Shell-owned panel: opening/closing never navigates or flushes editor buffers. */
export function AgentAuthorizationPanel() {
  const [open, setOpen] = useState(false);
  const [snapshot, setSnapshot] = useState(emptySnapshot);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [actionError, setActionError] = useState('');
  const [notice, setNotice] = useState('');
  const [now, setNow] = useState(Date.now);
  const seen = useRef(new Set<string>());
  const mounted = useRef(false);
  const mutationInFlight = useRef(false);
  const readInFlight = useRef(false);
  const generation = useRef(0);
  const available = agentAuthorizationClient.available();

  const accept = useCallback((next: AgentAuthorizationSnapshot) => {
    const fresh = unseenAgentRequests(next.pending, seen.current, Date.now());
    next.pending.forEach(request => seen.current.add(request.requestId));
    if (fresh.length) setOpen(true);
    setSnapshot(next);
    setNow(Date.now());
  }, []);

  const refresh = useCallback(async () => {
    if (!available || readInFlight.current || mutationInFlight.current) return;
    readInFlight.current = true;
    const revision = generation.current;
    try {
      const next = await agentAuthorizationClient.command({ action: 'list' });
      if (mounted.current && revision === generation.current) { accept(next); setError(''); }
    } catch {
      // No raw IPC error rendering: error text may contain upstream request data.
      if (mounted.current && revision === generation.current) setError('无法读取外部工具授权，请重试并检查桌面宿主是否已就绪。');
    } finally { readInFlight.current = false; }
  }, [accept, available]);

  useEffect(() => {
    mounted.current = true;
    const unsubscribe = subscribeAgentAuthorizationOpen(() => { setOpen(true); void refresh(); });
    void refresh();
    const timer = window.setInterval(() => { setNow(Date.now()); void refresh(); }, 3000);
    return () => { mounted.current = false; generation.current += 1; unsubscribe(); window.clearInterval(timer); };
  }, [refresh]);

  const act = async (command: Exclude<AgentAuthorizationAction, { action: 'list' }>) => {
    if (mutationInFlight.current) return;
    mutationInFlight.current = true;
    generation.current += 1;
    setBusy(true); setActionError(''); setNotice('');
    try {
      const next = await agentAuthorizationClient.command(command);
      if (mounted.current) {
        accept(next);
        setNotice(command.action === 'approve' ? '已授权，请返回终端继续。' : command.action === 'deny' ? '已拒绝此请求。' : '已撤销，此连接不能再发起操作。已提交任务不会自动取消。');
      }
    } catch {
      if (mounted.current) setActionError('授权操作未确认成功，请刷新状态后重试。请求可能已到期，或桌面宿主暂不可用。');
    } finally {
      mutationInFlight.current = false;
      if (mounted.current) setBusy(false);
      void refresh();
    }
  };

  const pendingCount = snapshot.pending.filter(request => canApproveAgentRequest(request, now)).length;
  return <>
    <Badge count={pendingCount} size="small">
      <Button type="text" onClick={() => { setOpen(true); void refresh(); }} aria-label={`外部工具授权${pendingCount ? `，${pendingCount} 个待确认请求` : ''}`}>
        外部工具{snapshot.legacyTrustedDevelopment === true ? ' · 开发协议警告' : error ? ' · 连接异常' : ''}
      </Button>
    </Badge>
    <AppDrawer title="外部工具授权" open={open} onClose={() => setOpen(false)} width="min(520px, 100vw)" mask={false} autoFocus={false}>
      <Space direction="vertical" size={16} style={{ width: '100%' }}>
        {snapshot.legacyTrustedDevelopment === true && <Alert type="warning" showIcon message="旧版开发协议已开启" description="旧入口不受本面板的授权、草稿范围限制与撤销控制。正常使用请关闭 AISTUDIO_AGENT_LEGACY_V1 并重启应用。" />}
        <Paragraph>在 Skill 或 OpenCode 的终端发起登录，然后在这里确认一次授权。授权范围内的普通编辑不会重复弹窗。</Paragraph>
        <Alert type="info" showIcon message="核对终端中的确认码" description="应用名称与版本由请求方自行声明，不代表身份已验证。只批准你刚刚发起、且确认码一致的连接。" />
        {!available && <Alert type="warning" message="请在小熊AI桌面应用中管理外部工具授权。" />}
        {error && <Alert type="error" message={error} action={<Button size="small" disabled={busy} onClick={() => void refresh()}>刷新状态</Button>} />}
        {actionError && <Alert type="error" message={actionError} />}
        {notice && <Alert type="success" message={notice} />}
        <div><Text strong>待确认请求</Text></div>
        {!snapshot.pending.length && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无待确认请求；请先在终端发起登录" />}
        {snapshot.pending.map(request => {
          const canApprove = canApproveAgentRequest(request, now);
          return <Card key={request.requestId} size="small" title={<span style={{ overflowWrap: 'anywhere' }}>{request.client.name} · {request.client.version}</span>}>
            <Paragraph type="secondary" style={{ overflowWrap: 'anywhere' }}>客户端 ID：{request.client.id}（自行声明）</Paragraph>
            <Paragraph>确认码：<Text code strong>{request.userCode}</Text></Paragraph>
            <ScopeSummary scopes={request.scopes} draftIds={request.draftIds} />
            <Paragraph><Tag color={canApprove ? 'blue' : 'default'}>{canApprove ? '等待确认' : request.expiresAt <= now ? '已到期' : request.status}</Tag>有效至 {new Date(request.expiresAt).toLocaleString()}</Paragraph>
            <Space wrap>
              <Button disabled={busy || !canApprove} onClick={() => void act({ action: 'deny', requestId: request.requestId })}>拒绝</Button>
              <Button type="primary" disabled={busy || !canApprove} onClick={() => void act({ action: 'approve', requestId: request.requestId })}>允许连接</Button>
            </Space>
          </Card>;
        })}
        <div><Text strong>已授权连接</Text></div>
        {!snapshot.grants.length && <Paragraph type="secondary">暂无已授权连接。</Paragraph>}
        {snapshot.grants.map(grant => <Card key={grant.sessionId} size="small" title={<span style={{ overflowWrap: 'anywhere' }}>{grant.client.name} · {grant.client.version}</span>}>
          <Paragraph type="secondary" style={{ overflowWrap: 'anywhere' }}>客户端 ID：{grant.client.id}（自行声明）</Paragraph>
          <ScopeSummary scopes={grant.scopes} draftIds={grant.draftIds} />
          <Paragraph><Tag color={grant.expiresAt > now ? 'green' : 'default'}>{grant.expiresAt > now ? '已授权' : '已到期'}</Tag>有效至 {new Date(grant.expiresAt).toLocaleString()}</Paragraph>
          <Button danger disabled={busy} onClick={() => void act({ action: 'revoke', sessionId: grant.sessionId })}>撤销授权</Button>
        </Card>)}
      </Space>
    </AppDrawer>
  </>;
}

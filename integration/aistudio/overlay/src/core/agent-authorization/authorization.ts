/** Public projection only. Never add token, challenge verifier or credential fields here. */
export interface AgentClientIdentity { id: string; name: string; version: string }
export interface AgentAuthorizationGrant {
  sessionId: string;
  client: AgentClientIdentity;
  scopes: string[];
  draftIds: string[];
  expiresAt: number;
}
export interface AgentAuthorizationRequest extends Omit<AgentAuthorizationGrant, 'sessionId'> {
  requestId: string;
  userCode: string;
  status: string;
}
export interface AgentAuthorizationSnapshot {
  legacyTrustedDevelopment?: boolean;
  pending: AgentAuthorizationRequest[];
  grants: AgentAuthorizationGrant[];
}
export const agentScopeLabels: Readonly<Record<string, string>> = {
  read: '读取草稿与任务',
  edit: '编辑草稿',
  generate: '提交生成任务（可能产生渠道费用）',
  export: '导出作品',
  'channels.read': '读取渠道配置（不含私钥）',
  'channels.write': '新增与修改渠道配置',
  'channels.credentials.write': '写入渠道私钥（仅写入，不能读取已有私钥）'
};
export function agentDraftScopeLabel(draftIds: readonly string[]): string {
  if (draftIds.includes('*')) return '全部草稿';
  return draftIds.length ? draftIds.join('、') : '不访问任何草稿';
}
export function canApproveAgentRequest(request: AgentAuthorizationRequest, now: number): boolean {
  return request.status === 'pending' && request.expiresAt > now;
}
export function unseenAgentRequests(requests: readonly AgentAuthorizationRequest[], seen: ReadonlySet<string>, now: number): string[] {
  return requests.filter(request => canApproveAgentRequest(request, now) && !seen.has(request.requestId)).map(request => request.requestId);
}

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { agentDraftScopeLabel, agentScopeLabels, canApproveAgentRequest, unseenAgentRequests } from './authorization.ts';

const request = { requestId: 'request-one', client: { id: 'fixture', name: 'Test client', version: '1' }, userCode: 'TEST-CODE', scopes: ['edit'], draftIds: ['draft-one'], expiresAt: 2000, status: 'pending' };
test('authorization distinguishes all drafts, no drafts, and explicit draft IDs', () => {
  assert.equal(agentDraftScopeLabel([]), '不访问任何草稿');
  assert.equal(agentDraftScopeLabel(['*']), '全部草稿');
  assert.equal(agentDraftScopeLabel(['draft-one']), 'draft-one');
  assert.match(agentScopeLabels['channels.credentials.write'], /仅写入，不能读取/);
});
test('expiry and dismissed request do not re-open; a new request is discoverable', () => {
  assert.equal(canApproveAgentRequest(request, 1999), true);
  assert.equal(canApproveAgentRequest(request, 2000), false);
  assert.equal(canApproveAgentRequest({ ...request, status: 'denied' }, 1000), false);
  assert.deepEqual(unseenAgentRequests([request], new Set(), 1000), ['request-one']);
  assert.deepEqual(unseenAgentRequests([request], new Set(['request-one']), 1000), []);
  assert.deepEqual(unseenAgentRequests([request, { ...request, requestId: 'request-two' }], new Set(['request-one']), 1000), ['request-two']);
});
test('real shell mounts panel; settings opens it without route changes or public HTTP approval', () => {
  const app = readFileSync(new URL('../../App.tsx', import.meta.url), 'utf8');
  const settings = readFileSync(new URL('../../pages/SettingsPage.tsx', import.meta.url), 'utf8');
  const client = readFileSync(new URL('../../store/agentAuthorizationClient.ts', import.meta.url), 'utf8');
  const panel = readFileSync(new URL('../../features/agent-authorization/AgentAuthorizationPanel.tsx', import.meta.url), 'utf8');
  assert.match(app, /<AgentAuthorizationPanel \/>/);
  assert.match(settings, /onClick=\{openAgentAuthorization\}/);
  assert.match(client, /invoke\('agent_authorization', command\)/);
  assert.doesNotMatch(client, /fetch\(/);
  assert.match(panel, /mask=\{false\} autoFocus=\{false\}/);
  assert.doesNotMatch(panel, /navigate\(|localStorage|JSON.stringify/);
});

import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AgentAuthorizationSnapshot } from '../core/agent-authorization/authorization';

export type AgentAuthorizationAction =
  | { action: 'list' }
  | { action: 'approve' | 'deny'; requestId: string }
  | { action: 'revoke'; sessionId: string };

/** Approval is a trusted main-WebView command, never a loopback HTTP endpoint. */
export const agentAuthorizationClient = {
  available: () => isTauri(),
  command: (command: AgentAuthorizationAction): Promise<AgentAuthorizationSnapshot> => invoke('agent_authorization', command)
};

const OPEN_EVENT = 'aistudio:agent-authorization';
export function openAgentAuthorization(): void { window.dispatchEvent(new Event(OPEN_EVENT)); }
export function subscribeAgentAuthorizationOpen(listener: () => void): () => void {
  window.addEventListener(OPEN_EVENT, listener);
  return () => window.removeEventListener(OPEN_EVENT, listener);
}

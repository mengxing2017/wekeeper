// Browser facade; other consumers inject a separate transport.
import {requestResource,requestCollection} from '../../http';
import {createWecomDistributionClient} from './client';
export * from './client';
export const {broadcast,officialSenders,distributionPolicies}=createWecomDistributionClient({requestResource,requestCollection});

export * from '../product-workflows'

/** Wecom permissions are independent of school memberships. */
export interface WecomWorkspace { id: string; name: string; role: 'admin' | 'operator' }
export interface WecomWorkspaceMember { id: string; username: string; display_name: string; role: 'admin' | 'operator' }
export const wecomWorkspaces = {
  list: () => requestCollection<WecomWorkspace>('/api/v1/wecom-distribution/workspaces'),
  save: (id: string, name: string) => requestResource<WecomWorkspace>(`/api/v1/wecom-distribution/workspaces/${encodeURIComponent(id)}`, 'PUT', { name }),
  members: (id: string) => requestCollection<WecomWorkspaceMember>(`/api/v1/wecom-distribution/workspaces/${encodeURIComponent(id)}/members`),
  grant: (id: string, username: string, role: 'admin' | 'operator' | 'revoked') => requestResource<{id:string;role:string}>(`/api/v1/wecom-distribution/workspaces/${encodeURIComponent(id)}/members`, 'PUT', { username, role }),
}

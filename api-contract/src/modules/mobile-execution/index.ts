import { requestCollection, requestResource, type Collection } from '../../http'
export interface MobileTarget {
  id: string; school_id: string; search_text: string; alias: string; remark: string;
  enabled: boolean; metadata_revision: number; locator_revision: number; archived_at: string | null;
}
export interface TargetInput {
  search_text: string; alias?: string; remark?: string;
  duplicate_confirmation?: string;
}
export interface DuplicateConfirmation {
  id: string; confirmation_required: true; duplicate_confirmation: string; candidate_ids: string[];
}
export interface MobileDevice {
  id: string; name: string; enabled: boolean; last_seen_at: string | null;
  control_state: 'online' | 'connecting' | 'suspect' | 'offline' | 'unauthorized' | 'disabled' | null;
  last_control_heartbeat_at: string | null;
}
export interface PlatformDevice {
  id: string; name: string; enabled: boolean; last_seen_at: string | null;
  remark: string; custodian_actor_id: string | null; metadata_revision: number;
}
export interface PlatformTarget {
  id: string; school_id: string; school_name: string; search_text: string; alias: string; enabled: boolean;
}
export interface SharedBindingInput {
  target_id: string; device_ids: string[]; search_text_override: string | null;
}
export interface TargetBinding {
 source?: 'platform' | 'school_pool';
  id: string; school_id: string; device_id: string; target_id: string;
  search_text_override: string | null; locator_revision: number;
  target_locator_revision?: number;
  search_source?: 'target_default' | 'binding_override'; source_locator_revision?: number;
  enabled: boolean; revision: number;
  school_name?: string; device_name?: string; search_text?: string; target_alias?: string; available?: boolean;
}
const base = (school: string) => `/api/v1/schools/${encodeURIComponent(school)}/mobile-execution`
function write<T>(path: string, method: string, attributes: unknown, key: string): Promise<T> {
  if (!key.trim() || [...key].length > 128) throw new Error('请提供有效的幂等键')
  return requestResource<T>(path, method, attributes, undefined, { 'Idempotency-Key': key })
}
function list<T>(path: string, page = 1, search = '', deviceId?: string): Promise<Collection<T>> {
  return requestCollection<T>(`${path}?page[number]=${page}&page[size]=30&search=${encodeURIComponent(search)}${deviceId ? `&device_id=${encodeURIComponent(deviceId)}` : ''}`)
}
export const mobileExecutionApi = {
  target: (school: string, id: string) => requestResource<MobileTarget>(`${base(school)}/targets/${encodeURIComponent(id)}`),
  platformDevices: (page?: number) => list<PlatformDevice>('/api/v1/platform/mobile-execution/devices', page),
  targets: (school: string, page?: number, search?: string, deviceId?: string) => list<MobileTarget>(`${base(school)}/targets`, page, search, deviceId),
  devices: (school: string, page?: number) => list<MobileDevice>(`${base(school)}/devices`, page),
  bindings: (school: string, page?: number, deviceId?: string) => list<TargetBinding>(`${base(school)}/target-bindings`, page, '', deviceId),
  createTarget: (school: string, input: TargetInput, key: string) => write<MobileTarget | DuplicateConfirmation>(`${base(school)}/targets`, 'POST', input, key),
  updateTarget: (school: string, id: string, input: Pick<MobileTarget, 'search_text' | 'alias' | 'remark' | 'enabled'> & { expected_revision: number }, key: string) => write<MobileTarget>(`${base(school)}/targets/${encodeURIComponent(id)}`, 'PATCH', input, key),
  archiveTarget: (school: string, id: string, expected_revision: number, reason: string, key: string) => write<MobileTarget>(`${base(school)}/targets/${encodeURIComponent(id)}`, 'DELETE', { expected_revision, reason }, key),
  platformTargets: (page?: number, search?: string) => list<PlatformTarget>('/api/v1/platform/mobile-execution/targets', page, search),
  platformBindings: (page?: number, search?: string, deviceId?: string) => list<TargetBinding>('/api/v1/platform/mobile-execution/target-bindings', page, search, deviceId),
  bind: (input: SharedBindingInput, key: string) => write<{ id: string; school_id: string; target_id: string; bindings: TargetBinding[] }>('/api/v1/platform/mobile-execution/target-bindings', 'POST', input, key),
  updateBinding: (id: string, input: { expected_revision: number; enabled: boolean; search_text_override: string | null }, key: string) => write<TargetBinding>(`/api/v1/platform/mobile-execution/target-bindings/${encodeURIComponent(id)}`, 'PATCH', input, key),
  unbind: (id: string, expected_revision: number, reason: string, key: string) => write<TargetBinding>(`/api/v1/platform/mobile-execution/target-bindings/${encodeURIComponent(id)}/revocations`, 'POST', { expected_revision, reason }, key),
}

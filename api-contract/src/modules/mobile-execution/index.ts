import { requestCollection, requestResource, type Collection } from '../../http'
export interface MobileTarget {
  id: string; search_text: string; remark: string;
  enabled: boolean; metadata_revision: number; locator_revision: number; archived_at: string | null;
}
export interface TargetInput {
  search_text: string; remark?: string;
  duplicate_confirmation?: string;
}
export interface DuplicateConfirmation {
  id: string; confirmation_required: true; duplicate_confirmation: string;
}
export interface MobileDevice {
  id: string; name: string; enabled: boolean; last_seen_at: string | null;
}
export interface PlatformDevice {
  id: string; name: string; enabled: boolean; last_seen_at: string | null;
}
export interface PlatformTarget {
  id: string; search_text: string; enabled: boolean;
}
export interface SharedBindingInput {
  target_id: string; device_ids: string[];
}
export interface TargetBinding {
  max_calls?:number|null;valid_until?:string|null;used_calls?:number;
  id: string; device_id: string; target_id: string;
  locator_revision: number;
  target_locator_revision?: number;
  enabled: boolean; revision: number; device_name?: string; search_text?: string; available?: boolean;
}
const base = () => '/api/v1/mobile-execution'
function write<T>(path: string, method: string, attributes: unknown, key: string): Promise<T> {
  if (!key.trim() || [...key].length > 128) throw new Error('请提供有效的幂等键')
  return requestResource<T>(path, method, attributes, undefined, { 'Idempotency-Key': key })
}
function list<T>(path: string, page = 1, search = '', deviceId?: string): Promise<Collection<T>> {
  return requestCollection<T>(`${path}?page[number]=${page}&page[size]=30&search=${encodeURIComponent(search)}${deviceId ? `&device_id=${encodeURIComponent(deviceId)}` : ''}`)
}
export const mobileExecutionApi = {
  target: (id: string) => requestResource<MobileTarget>(`${base()}/targets/${encodeURIComponent(id)}`),
  platformDevices: (page?: number) => list<PlatformDevice>('/api/v1/platform/mobile-execution/devices', page),
  targets: (page?: number, search?: string, deviceId?: string) => list<MobileTarget>(`${base()}/targets`, page, search, deviceId),
  devices: (page?: number) => list<MobileDevice>(`${base()}/devices`, page),
  bindings: (page?: number, deviceId?: string) => list<TargetBinding>(`${base()}/target-bindings`, page, '', deviceId),
  createTarget: (input: TargetInput, key: string) => write<MobileTarget | DuplicateConfirmation>(`${base()}/targets`, 'POST', input, key),
  updateTarget: (id: string, input: Pick<MobileTarget, 'search_text' | 'remark' | 'enabled'> & { expected_revision: number }, key: string) => write<MobileTarget>(`${base()}/targets/${encodeURIComponent(id)}`, 'PATCH', input, key),
  archiveTarget: (id: string, expected_revision: number, reason: string, key: string) => write<MobileTarget>(`${base()}/targets/${encodeURIComponent(id)}`, 'DELETE', { expected_revision, reason }, key),
  platformTargets: (page?: number, search?: string) => list<PlatformTarget>('/api/v1/platform/mobile-execution/targets', page, search),
  platformBindings: (page?: number, search?: string, deviceId?: string) => list<TargetBinding>('/api/v1/platform/mobile-execution/target-bindings', page, search, deviceId),
  bind: (input: SharedBindingInput, key: string) => write<{ id: string; target_id: string; bindings: TargetBinding[] }>('/api/v1/platform/mobile-execution/target-bindings', 'POST', input, key),
  updateBinding: (id: string, input: { expected_revision: number; enabled: boolean;max_calls:number|null;valid_until:string|null }, key: string) => write<TargetBinding>(`/api/v1/platform/mobile-execution/target-bindings/${encodeURIComponent(id)}`, 'PATCH', input, key),
  unbind: (id: string, expected_revision: number, reason: string, key: string) => write<TargetBinding>(`/api/v1/platform/mobile-execution/target-bindings/${encodeURIComponent(id)}/revocations`, 'POST', { expected_revision, reason }, key),
}

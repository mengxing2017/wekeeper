import type { FlowV2, FlowVersion, FlowValidation, FlowRelease, ExecutionAssignment, AssignmentInput } from './flows'
export * from './flows'
import { requestCollection, requestResource, type Collection } from '../../http'
export interface MobileTarget {
  id: string; school_id: string; search_text: string; alias: string; remark: string;
  enabled: boolean; metadata_revision: number; locator_revision: number; archived_at: string | null;
}
export interface TargetInput {
  search_text: string; alias?: string; remark?: string; device_ids?: string[];
  duplicate_confirmation?: string;
}
export interface DuplicateConfirmation {
  id: string; confirmation_required: true; duplicate_confirmation: string; candidate_ids: string[];
}
export interface MobileDevice {
  id: string; name: string; enabled: boolean; last_seen_at: string | null;
  grant_id: string; grant_revision: number; school_alias: string; school_remark: string;
  control_state: 'online' | 'connecting' | 'suspect' | 'offline' | 'unauthorized' | 'disabled';
  last_control_heartbeat_at: string | null;
}
export interface PlatformDevice {
  id: string; name: string; enabled: boolean; last_seen_at: string | null;
  remark: string; custodian_actor_id: string | null; metadata_revision: number;
}
export interface DeviceGrant {
  id: string; school_id: string; device_id: string; purpose: string; enabled: boolean; revision: number;
  device_name?: string; school_name?: string;
}
export interface TargetBinding {
  id: string; school_id: string; device_id: string; target_id: string; grant_id: string;
  enabled: boolean; revision: number;
  device_name?: string; search_text?: string; target_alias?: string; available?: boolean;
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
  flows: (school: string, page?: number) => list<FlowVersion>(`${base(school)}/flow-versions`, page),
  createFlow: (school: string, input: { name: string; flow: FlowV2 }, key: string) => write<FlowVersion>(`${base(school)}/flow-versions`, 'POST', input, key),
  validations: (school: string, page?: number, deviceId?: string) => list<FlowValidation>(`${base(school)}/flow-validations`, page, '', deviceId),
  validateFlow: (school: string, input: { flow_version_id: string; device_id: string; search_text: string }, key: string) => write<FlowValidation>(`${base(school)}/flow-validations`, 'POST', input, key),
  releases: (school: string, page?: number) => list<FlowRelease>(`${base(school)}/flow-releases`, page),
  releaseFlow: (school: string, input: { flow_version_id: string; validation_id: string; name: string }, key: string) => write<FlowRelease>(`${base(school)}/flow-releases`, 'POST', input, key),
  revokeRelease: (school: string, id: string, expected_revision: number, reason: string, key: string) => write<FlowRelease>(`${base(school)}/flow-releases/${encodeURIComponent(id)}/revocations`, 'POST', { expected_revision, reason }, key),
  assignments: (school: string, page?: number) => list<ExecutionAssignment>(`${base(school)}/assignments`, page),
  assign: (school: string, input: AssignmentInput, key: string) => write<ExecutionAssignment>(`${base(school)}/assignments`, 'POST', input, key),
  updateAssignment: (school: string, id: string, input: AssignmentInput, key: string) => write<ExecutionAssignment>(`${base(school)}/assignments/${encodeURIComponent(id)}`, 'PATCH', input, key),
  target: (school: string, id: string) => requestResource<MobileTarget>(`${base(school)}/targets/${encodeURIComponent(id)}`),
  platformDevices: (page?: number) => list<PlatformDevice>('/api/v1/platform/mobile-execution/devices', page),
  targets: (school: string, page?: number, search?: string, deviceId?: string) => list<MobileTarget>(`${base(school)}/targets`, page, search, deviceId),
  devices: (school: string, page?: number) => list<MobileDevice>(`${base(school)}/devices`, page),
  bindings: (school: string, page?: number, deviceId?: string) => list<TargetBinding>(`${base(school)}/target-bindings`, page, '', deviceId),
  createTarget: (school: string, input: TargetInput, key: string) => write<MobileTarget | DuplicateConfirmation>(`${base(school)}/targets-with-bindings`, 'POST', input, key),
  updateTarget: (school: string, id: string, input: Pick<MobileTarget, 'search_text' | 'alias' | 'remark' | 'enabled'> & { expected_revision: number }, key: string) => write<MobileTarget>(`${base(school)}/targets/${encodeURIComponent(id)}`, 'PATCH', input, key),
  archiveTarget: (school: string, id: string, expected_revision: number, reason: string, key: string) => write<MobileTarget>(`${base(school)}/targets/${encodeURIComponent(id)}`, 'DELETE', { expected_revision, reason }, key),
  bind: (school: string, target_id: string, device_ids: string[], key: string) => write<{ id: string }>(`${base(school)}/target-bindings`, 'POST', { target_id, device_ids }, key),
  unbind: (school: string, id: string, expected_revision: number, reason: string, key: string) => write<TargetBinding>(`${base(school)}/target-bindings/${encodeURIComponent(id)}/revocations`, 'POST', { expected_revision, reason }, key),
  grants: (page?: number) => list<DeviceGrant>('/api/v1/platform/mobile-execution/device-school-grants', page),
  grant: (input: Pick<DeviceGrant, 'school_id' | 'device_id' | 'purpose'>, key: string) => write<DeviceGrant>('/api/v1/platform/mobile-execution/device-school-grants', 'POST', input, key),
  revokeGrant: (id: string, expected_revision: number, reason: string, key: string) => write<DeviceGrant>(`/api/v1/platform/mobile-execution/device-school-grants/${encodeURIComponent(id)}/revocations`, 'POST', { expected_revision, reason }, key),
}

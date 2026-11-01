import type { FlowV2 } from './flows'
export type SharedFlowV2 = Omit<FlowV2, 'protocol_version'> & { protocol_version: 7 }
export interface DeviceEnvironment {
  id: string; device_id: string; credential_generation: number; digest: string;
  snapshot: { android_api: number; engine_version: string; application_status: 'confirmed' | 'unconfirmed' | 'unsupported_instances'; package_name: string; version_name: string | null; version_code: number | null; instance_ref: string | null; launcher_package: string | null; launcher_version: number | null };
}
export interface AdapterRelease {
  id: string; name: string; version: string; owner: string; source_revision: string;
  content_digest: string; published_at: string; evidence: unknown[]; environment: Omit<DeviceEnvironment['snapshot'], 'instance_ref'>;
}
export interface CapabilityOpeningInput {
  device_id: string; profile_release_id: string; environment_snapshot_id: string; expected_revision: number;
}
export interface CapabilityOpeningApplication {
  id: string; device_id: string; capability_id: string; configuration_revision: number;
  release_id: string; environment_snapshot_id: string; revision: number; state: 'awaiting_consent' | 'cancelled';
}
export interface AdapterSupportRequest {
  id: string; device_id: string; note: string; owner: string; revision: number;
  state: 'submitted' | 'investigating' | 'resolved' | 'declined'; response: string | null; created_at: string;
}
export interface CapabilityOpening {
  id: string; device_name: string; device_enabled: boolean; online: boolean | null;
  environment: DeviceEnvironment | null; observed_at: string | null; environment_fresh: boolean;
  configuration_revision: number; opening: CapabilityOpeningApplication | null;
  release_count: number; matching_releases: AdapterRelease[]; release_available: boolean;
  field_check_current_passed?: boolean; consent_current: boolean; local_device_state: 'unconfirmed';
  delivery_state: 'onboarding_only'; execution_ready: false; can_open: boolean;
  support_request: AdapterSupportRequest | null;
  blockers: Array<{code: string; message: string; owner: string; next_action: string}>;
}
export interface SharedDeviceScope {
  schema: 'shared_mobile_execution_scope/1'; platform_source: 'wekeeper'; device_id: string;
  credential_generation: number; capability_id: string; configuration_revision: number;
  configuration_digest: string; configuration_name: string; app_key: 'wecom'; capability: 'group_text';
}
export interface DeviceCapabilityConsent {
  id: string; device_id: string; capability_id: string; configuration_revision: number;
  credential_generation: number; scope_digest: string; scope: SharedDeviceScope;
  revision: number; accepted: boolean; updated_at: string;
}
export interface DeviceCapabilityContext {
  id: string; protocol_version: 7;
  scopes: Array<{id: string; scope: SharedDeviceScope; scope_digest: string; consent: DeviceCapabilityConsent | null; accepted: boolean}>;
}
export interface DeviceCapabilityDecision {
  capability_id: string; configuration_revision: number; scope_digest: string; expected_revision: number; accepted: boolean;
}

/** One school-authorized target inspection; never a sending authorization. */
export interface FieldCheckInvitation {
  id: string; school_id: string; device_id: string; target_id: string; binding_id: string;
  binding_revision: number; target_locator_revision: number; search_text: string;
  revision: number; revoked_at: string | null; expires_at: string; available?: boolean; consumed?: boolean; school_name?: string;
}
export interface FieldCheckRequest {
  opening_id: string; invitation_id: string; expected_invitation_revision: number;
}
export interface FieldCheck {
  id: string; device_id: string; opening_id: string; school_id: string; invitation_id: string;
  state: 'pending' | 'leased' | 'passed' | 'failed' | 'stale' | 'expired'; revision: number;
  configuration_revision: number; consent_revision: number; environment_snapshot_id: string; dependencies_current?: boolean;
  credential_generation: number; expires_at: string;
}
export interface FieldCheckClaim {
  id: string; device_id: string; credential_generation: number; lease_fence: number; challenge: string;
  timing: {server_time: string; expires_at: string}; flow: SharedFlowV2; flow_digest: string;
  environment: DeviceEnvironment['snapshot']; capability_id: string; configuration_revision: number;
  scope_digest: string; consent_revision: number; search_text: string;
}
export interface FieldCheckResultInput {
  challenge: string; lease_fence: number; evidence: string;
  checks: Record<'application_opened' | 'group_container_only' | 'first_group_visual_order' | 'results_stable' | 'chat_opened' | 'send_control_unique' | 'human_pacing' | 'no_send_performed', boolean>;
}
export interface FieldCheckReceipt {id: string; disposition: 'passed' | 'failed' | 'stale'; body: FieldCheckResultInput}

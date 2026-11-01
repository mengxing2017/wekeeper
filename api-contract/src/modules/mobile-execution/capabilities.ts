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
  consent_current: boolean; local_device_state: 'unconfirmed';
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

import type { FlowV2 } from './flows'
export type SharedFlowV2 = Omit<FlowV2, 'protocol_version'> & { protocol_version: 7 }
export interface DeviceCapabilityConfigurationInput {
  device_id: string; expected_revision: number; name: string; enabled: boolean; flow: SharedFlowV2;
}
export interface DeviceCapabilityConfiguration {
  id: string; device_id: string; app_key: 'wecom'; capability: 'group_text';
  name: string; revision: number; enabled: boolean; configuration_digest: string; flow?: SharedFlowV2;
  execution_ready: false; execution_block_reason: 'shared_execution_upgrade_required';
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

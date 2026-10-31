export interface UiPredicate {
  text?: string | null; description?: string | null; class_name?: string | null;
  visible: boolean; enabled?: boolean | null; editable?: boolean | null; clickable?: boolean | null;
}
export interface UiSelector { predicate: UiPredicate; ancestor?: UiPredicate | null; ancestor_depth: number }
export interface FlowV2 {
  schema_version: 2; engine_version: 'otter-ui/2'; protocol_version: 6;
  app_key: 'wecom'; capability: 'group_text'; application_label: string; package_name: string;
  selection_policy: 'first_group_result'; result_order: 'top_to_bottom'; results_stable_ms: number; page_timeout_ms: number;
  pages: Record<string, UiSelector>; controls: Record<string, UiSelector>;
  navigation: Array<{ from: string; to: string; control: string; delay_ms: number }>;
  launchers: Array<{ package_name: string; search_input: UiSelector; application_row: UiSelector; max_scrolls: number }>;
  pacing: { navigation_min_ms: number; before_send_min_ms: number; between_targets_min_ms: number; baseline_evidence: string };
  max_attempts: number; max_total_steps: number; max_total_scrolls: number; max_total_duration_ms: number; max_targets: number;
}
export interface FlowVersion { id: string; school_id: string; name: string; flow: FlowV2; content_digest: string; created_at: string }
export const flowValidationChecks = ['application_opened', 'group_container_only', 'first_group_visual_order', 'results_stable', 'chat_opened', 'send_control_unique', 'human_pacing', 'no_send_performed'] as const
export interface FlowValidationResultInput {
  challenge: string; flow_digest: string; protocol_version: 6; engine_version: 'otter-ui/2';
  package_name: string; version_code: number; checks: Record<typeof flowValidationChecks[number], boolean>; evidence: string;
}
export interface FlowValidation {
  id: string; school_id: string; device_id: string; flow_version_id: string;
  grant_id: string; grant_revision: number; consent_id: string; consent_revision: number; search_text: string;
  state: 'pending' | 'passed' | 'failed'; expires_at: string; completed_at: string | null;
  flow_name?: string; device_name?: string; expired?: boolean;
  result: Omit<FlowValidationResultInput, 'challenge'> | null;
}
export interface DeviceFlowValidation extends FlowValidation { challenge: string; flow: FlowV2; flow_digest: string; school_name: string }
export interface FlowRelease {
  id: string; school_id: string; flow_version_id: string; validation_id: string; name: string;
  enabled: boolean; revision: number; flow_name?: string; flow_digest?: string;
}
export interface ExecutionAssignment {
  id: string; school_id: string; device_id: string; grant_id: string; release_id: string; enabled: boolean;
  execution_revision: number; metadata_revision: number; device_name?: string; release_name?: string;
  consent_required?: boolean; device_validation_required?: boolean; available?: boolean;
}
export interface AssignmentInput { device_id: string; release_id: string; expected_revision: number; enabled: boolean }

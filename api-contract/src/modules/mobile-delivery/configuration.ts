import type { UUID, Row, MobileItem } from "./index";
export interface UiPredicate {
  text?: string | null;
  description?: string | null;
  class_name?: string | null;
  visible?: boolean | null;
  enabled?: boolean | null;
  editable?: boolean | null;
  clickable?: boolean | null;
}
export interface UiSelector {
  predicate: UiPredicate;
  parent?: UiPredicate | null;
  ancestor?: UiPredicate | null;
  ancestor_depth: number;
  all_of?: UiPredicate[];
  any_of?: UiPredicate[];
}
export interface UiClient {
  package_name: string;
  version_name: string;
  version_code: number;
}
export interface UiRules {
  schema_version: number;
  engine_version: string;
  required_capabilities: string[];
  client: UiClient;
  pages: Record<
    string,
    { marker: UiSelector; required: UiSelector[]; forbidden: UiSelector[] }
  >;
  controls: Record<string, UiSelector>;
  navigation_graph: Array<{
    from: string;
    to: string;
    control: string;
    min_delay_ms: number;
  }>;
  landmarks: {
    entry: string;
    account: string;
    search: string;
    results: string;
    chat: string;
  };
  group_type_labels: Record<"internal" | "external", string>;
  member_count_suffix: "none" | "ascii" | "full_width";
}
export interface ExecutionPolicy {
  account_cooldown_ms: number;
  content_cooldown_ms: number;
  default_group_interval_ms: number;
  minimum_group_interval_ms: number;
  session_max_duration_ms: number;
  quota_window_ms: number;
  account_limit: number;
  group_limit: number;
  max_targets: number;
  input_codepoint_delay_ms: number;
  input_settle_ms: number;
  page_timeout_ms: number;
  observation_timeout_ms: number;
  poll_interval_ms: number;
  max_navigation_steps: number;
  minimum_manual_samples: number;
}
export interface ManualBaseline {
  version: string;
  samples: number;
  human_group_min_interval_ms: number;
  navigation_min_ms: number;
  input_codepoint_min_ms: number;
  input_settle_min_ms: number;
  observation_min_ms: number;
  evidence_ref: string;
}
export interface ConfigurationInput {
  name: string;
  rules: UiRules;
  policy: ExecutionPolicy;
  baseline: ManualBaseline;
  evidence_ref: string;
}
export interface ExecutionConfiguration extends Row {
  name: string;
  configuration: ConfigurationInput;
  content_digest: string;
  created_by: UUID;
  created_at: string;
}
export interface ConfigurationRelease extends Row {
  configuration_id: UUID;
  school_id: UUID;
  device_id: UUID | null;
  active: boolean;
  revoked: boolean;
  evidence_ref: string;
  created_at: string;
  name?: string;
  client?: UiClient;
  configuration?: ConfigurationInput;
  content_digest?: string;
}
export interface RunProgress extends Row {
  current_position: number;
  last_completed_at: string | null;
  next_eligible_at: string | null;
  pause_reason: string | null;
  configuration_release_id: UUID;
  execution_revision: number;
  inter_group_interval_ms: number;
  items: Array<MobileItem & { position: number }>;
  server_time: string;
}
export interface RunRevisionPreview {
  id: UUID;
  revision: number;
  preview_digest: string;
  progress: RunProgress;
  new_release: ConfigurationRelease;
}

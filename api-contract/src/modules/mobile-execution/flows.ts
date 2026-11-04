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

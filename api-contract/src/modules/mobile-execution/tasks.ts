import type { FlowV2 } from './flows'
export interface TaskPreviewInput { device_id: string; target_ids: string[]; message: string }
export interface TaskConfirmationInput { preview_id: string; preview_digest: string; confirmed: true }
export interface TaskItemSnapshot {
  target_id: string; search_text: string; locator_revision: number;
  binding_id: string; binding_revision: number; position: number;
}
export interface TaskPlanSnapshot {
  protocol_version: 6; school_id: string; message: string; selection_policy: 'first_group_result';
  configuration: {
    device_id: string; credential_generation: number; grant_id: string; grant_revision: number;
    consent_id: string; consent_revision: number; assignment_id: string; assignment_revision: number;
    release_id: string; release_revision: number; flow_version_id: string; flow_digest: string; flow: FlowV2;
  };
  items: TaskItemSnapshot[];
}
export interface TaskPreview {
  id: string; school_id: string; device_id: string; input: TaskPreviewInput; snapshot: TaskPlanSnapshot;
  digest: string; created_by: string; created_at: string; expires_at: string;
}
export type TaskItemState = 'pending' | 'claimed' | 'prepared' | 'invocation_issued' | 'reported_sent' | 'verified_not_sent' | 'unknown' | 'failed_before_send' | 'invalidated' | 'cancelled'
export interface ExecutionTask {
  id: string; school_id: string; intent_id: string; device_id: string; preview_id: string;
  state: 'queued' | 'running' | 'paused' | 'completed' | 'partially_completed' | 'cancelled'; revision: number;
  created_by: string; confirmed_by: string; created_at: string; cancelled_at: string | null;
}
export interface ExecutionTaskDetail extends ExecutionTask {
  device_name: string; plan: TaskPlanSnapshot; plan_digest: string;
  items: Array<{ id: string; school_id: string; task_id: string; intent_id: string; device_id: string; target_id: string; binding_id: string; position: number; attempt: number; snapshot: TaskItemSnapshot; state: TaskItemState }>;
}

/** Observations carry provenance, never executable credentials or new authorization. */
export interface ChannelObservation {
 id:number;channel:'wecom'|'mobile';workspace_id:string;task_id:string;target_id:string|null;attempt_id:string|null;batch_id:string|null;execution_id:string|null;invocation_id:string|null;
 evidence_type:string;observation_source:string;observed_at:string;device_occurred_at:string|null;external_status:string|null;normalized_result:string;detail:Record<string,unknown>;reconstructed:boolean;
}
export interface ExecutionCapability {
 execution_mode:string;target_kind:string;platform_interaction:'none'|'manual_task_confirmation';external_confirmation:'member_required'|'none';result_semantics:string;limits?:{max_batch_targets:number;max_content_bytes:number};
}
export const executionEvidenceLabels:Record<string,string>={platform_accepted:'平台已受理',official_task_created:'官方任务已创建，等待微信侧成员执行',external_member_unknown:'微信侧成员结果待核对',external_member_pending:'等待微信侧成员执行',external_member_executed:'微信侧成员已执行，逐群结果另行同步',official_group_sent:'官方报告本群已发送（不代表已读）',official_group_result:'官方逐群结果',official_read_failed:'官方结果同步失败，保留历史结果',official_submission:'官方提交进度',device_received:'手机已接收',device_observed_sent:'设备反馈发送成功（非官方送达凭证）',device_execution_result:'设备执行结果',operator_reconciled:'人工核对结论',unknown:'结果不确定'};

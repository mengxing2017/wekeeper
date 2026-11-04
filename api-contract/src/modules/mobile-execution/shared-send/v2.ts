import type { Client } from '../../../http/client'
import type { Json } from '../../business/types'
export const SHARED_SEND_SCHEMA = 'shared_mobile_send/2' as const
export interface SharedDraftInput { target_ids:string[]; message:string; content_type:'text/plain'; expected_revision?:number;between_targets_seconds:number }
export interface SharedDraft extends SharedDraftInput { targets:Array<{id:string;name:string}>;id:string; school_id:string; intent_id:string; revision:number; content_digest:string }
/** UI adaptation and local configuration are owned exclusively by the phone. */
export interface DeviceExecutionIntent {contract:'device_owned/1';capability:'wecom.group_text'}
export interface SharedCandidate {
 target_id:string; target_name:string;device_name:string;device_id:string; binding_id:string; binding_locator_revision:number;
 target_locator_revision:number|null; search_text:string; search_source:'default'|'override';
 consent_state?:'accepted'|'revoked'|'unknown';access_revision:number|null;credential_generation:number; delegation_id:string|null; /** Historical snapshots only. New tasks never carry UI rules. */ release_id?:string|null; flow_digest?:string|null;
 configuration_version?:string|null;scope_id?:string;confirmable?:boolean;flow?:Record<string,Json>|null;execution?:DeviceExecutionIntent;
}
export interface SharedPlan extends SharedCandidate {schema:typeof SHARED_SEND_SCHEMA;school_id:string;intent_id:string;item_id:string;attempt_id:string;message:string;content_digest:string;selection_policy:'first_group_result';between_targets_seconds:number;policy_id:string}
export interface SharedConfigurationIssue {code:string;path:string}
export interface SharedConfigurationStatus {capability:'wecom.group_text';available:boolean;configuration_id:string|null;version:string|null;flow_digest:string|null;issue:SharedConfigurationIssue|null;source:'platform_runtime_configuration';observed_at:string}
export interface SharedPreview { created_at?:string;id:string;digest:string;expires_at:string;plan:{configuration?:SharedConfigurationStatus;sending_status?:SendingStatus;schema:typeof SHARED_SEND_SCHEMA;school_id:string;draft_id:string;draft_revision:number;intent_id:string;message:string;content_digest:string;policy_id:string|null;items:Array<{position:number;target_id:string;target:{id:string;name:string;search_text:string;locator_revision:number};selected:SharedCandidate|null;candidates:SharedCandidate[];blocker:string|null;readiness?:SharedReadiness[]}>} }
export type SharedAttemptState='queued'|'leased'|'prepared'|'invocation_issued'|'observed_sent'|'unknown'|'failed_before_call'|'invalidated'|'cancelled'|'verified_not_sent'|'manually_confirmed_sent'
export interface SharedExecutionCondition {waiting_since?:string|null;reason_changed_at?:string|null;code:string;category:string;affected_action:string;scope_type:string;scope_id:string;owner:string;message:string;automatic:boolean;next_action:string;observed_at:string;valid_until:string|null;retry_at:string|null}
export interface SharedExecutionStatus {phase:'waiting'|'ready'|'executing'|'completed'|'blocked'|'unknown';observed_at:string;conditions:SharedExecutionCondition[];consent_observed_at:string|null;device_state_observed_at:string|null}
export interface SharedDelegationContext {runtime_report_version?:2;id:string;device_id:string;credential_generation:number;authorization_schema:typeof SHARED_DELEGATION_SCHEMA;delegation:{id:string;scope_digest:string;scope:SharedDelegationScope;decision:{accepted:boolean;revision:number}|null};observed_at:string}
export interface SharedAttempt {invalidated_at?:string|null;device_received_at?:string|null;execution_status?:SharedExecutionStatus;invalidation_reason?:string|null;result_reason?:string|null;id:string;state:SharedAttemptState;revision:number;plan:SharedPlan;plan_digest:string;attempt_no:number;current:boolean}
export interface SharedTask {id:string;school_id:string;intent_id:string;revision:number;stopped:boolean;created_at:string;confirmed_until?:string;cursor_position?:number;next_eligible_at?:string;items?:Array<{id:string;target_id:string;position:number;attempts:SharedAttempt[]}>}
export interface SharedReadiness {runtime_reason?:string|null;configuration_issue?:SharedConfigurationIssue|null;configuration_observed_at?:string;configuration_source?:string;configuration_id?:string|null;configuration_digest?:string|null;configuration_version?:string|null;preparation?:SharedDevicePreparation|null;consent_state?:'accepted'|'revoked'|'unknown';runtime_state?:'unknown'|'stale'|'paused'|'secure_locked'|'screen_off'|'keyguard_locked'|'accessibility_required'|'notification_required'|'ready'|'waiting';runtime_observed_at?:string|null;execution_conditions?:Array<{code:string;message:string;owner:string;automatic:boolean;action?:string}>;device_name:string;target_name:string;search_text:string;inspection_state:'waiting_prerequisites'|'not_scheduled'|'queued'|'running'|'passed'|'expired'|'failed'|'stale'|'retry_wait'|'retry_exhausted';retry_at:string|null;retry_remaining:number;binding_id:string;device_id:string;target_id:string;state:string;plan_selectable:boolean;confirmable:boolean;execution_ready:boolean;ready:boolean;automatic:boolean;blockers:Array<{code:string;message:string;owner:string;automatic:boolean}>;inspection:{id:string;state:string;expires_at:string}|null}
export interface SharedOverview {configuration?:SharedConfigurationStatus;sending_status?:SendingStatus;id:string;tasks:SharedTask[];drafts:SharedDraft[];readiness:SharedReadiness[];pool:Array<{id:string;name:string;access_revision:number}>;inspections:Array<{id:string;binding_id:string;state:string;expires_at:string}>}
export interface SharedLimit {interval_seconds:number;window_seconds:number;max_calls:number;daily_max_calls:number}
export interface SharedPolicyInput {expected_revision:number;mode:'disabled'|'draft_only'|'preview_only'|'enabled';confirmation_seconds:number;inspection_seconds:number;retry_seconds:number;retry_limit:number;timezone:string;allowed_windows:Array<{weekdays:number[];start_minute:number;end_minute:number}>;limits:{global:SharedLimit;school:SharedLimit;device:SharedLimit;target:SharedLimit};reason:string}
export interface SharedInspectionStart {inspection_id:string;runtime_epoch:string;lease_fence:number;challenge:string;dependencies_digest:string}
export interface SharedWakeDiagnostics {request_id:string;source:'accessibility_service';events:Array<{stage:string;elapsed_ms:number}>}
export interface SharedDevicePreparation {observed_at?:string|null;wake_diagnostics?:SharedWakeDiagnostics|null;automatic_wake_enabled:boolean;screen_awake_active:boolean;screen_awake_reason:string|null;stage:string;last_error:string|null;retry_after_ms:number}
export interface SharedScreenConditions extends Record<string,boolean> {interactive:boolean;keyguard_locked:boolean;device_locked:boolean;keyguard_secure:boolean;unlocked:boolean;accessibility:boolean;notification:boolean}
export interface SharedRuntimeReport {schema:typeof SHARED_SEND_SCHEMA;runtime_epoch:string;ready_reason?:string|null;ui_ready:boolean;paused:boolean;quiescent:boolean;ui_context_epoch:string;apk_sha256:string;conditions:SharedScreenConditions;preparation?:SharedDevicePreparation|null}
export interface SharedPreparationInput {attempt_id:string;lease_fence:number;plan_digest:string;content_digest:string;input_matches:boolean;page_digest:string;/** Legacy frozen plans only. */ flow_digest?:string}
export interface SharedResultEvent {result_credential:string|null;schema:typeof SHARED_SEND_SCHEMA;event_id:string;claim_id:string;attempt_id:string;invocation_id:string|null;credential_generation:number;lease_fence:number;plan_digest:string;content_digest:string;outcome:'observed_sent'|'unknown'|'failed_before_call';reason:string;consumed:boolean;calling_marked:boolean;evidence:string}
export interface SharedReceipt {id:string;event_id:string;claim_id:string;attempt_id:string;invocation_id:string|null;device_id:string;credential_generation:number;lease_fence:number;plan_digest:string;content_digest:string;body_hash:string;disposition:'observed_sent'|'unknown'|'failed_before_call'|'archived'}
export function createSharedSendV2Client(client:Pick<Client,'requestResource'>) {
 const root=(s:string)=>`/api/v1/schools/${encodeURIComponent(s)}/mobile-execution`
 const one=client.requestResource
 const post=<T>(path:string,input:unknown,key:string,method='POST')=>one<T>(path,method,input,undefined,{'Idempotency-Key':key})
 return {
  inspectionStarted:(input:SharedInspectionStart,key:string)=>post<{id:string;state:'started';dependencies_digest:string}>('/api/v9/device-execution/send-inspection-starts',input,key),
  delegationContext:()=>one<SharedDelegationContext>('/api/v9/device-execution/send-delegation-context'),
  saveBusinessScheme:(input:BusinessSchemeInput,key:string)=>post<{id:string;revision:number}>('/api/v1/platform/mobile-execution/business-schemes',input,key),
  overview:(s:string)=>one<SharedOverview>(`${root(s)}/shared-v2-tasks`),
  task:(s:string,id:string)=>one<SharedTask>(`${root(s)}/shared-v2-tasks/${encodeURIComponent(id)}`),
  confirm:(s:string,p:SharedPreview,key:string)=>post<SharedTask>(`${root(s)}/shared-v2-tasks`,{preview_id:p.id,preview_digest:p.digest,confirmed:true},key),
  stop:(s:string,id:string,expected_revision:number,reason:string,key:string)=>post<SharedTask>(`${root(s)}/shared-v2-tasks/${encodeURIComponent(id)}/stops`,{expected_revision,reason},key),
  review:(s:string,attempt:string,expected_revision:number,decision:'manually_confirmed_sent'|'verified_not_sent'|'still_unknown',evidence:string,key:string)=>post<{id:string}>(`${root(s)}/shared-v2-items/${encodeURIComponent(attempt)}/reviews`,{expected_revision,decision,evidence},key),
  inspection:(s:string,target_id:string,device_id:string,key:string)=>post<{id:string;state:string;readiness:SharedReadiness}>(`${root(s)}/shared-v2-inspections`,{target_id,device_id},key),
  reassign:(s:string,attempt:string,expected_revision:number,device_id:string,reason:string,key:string)=>post<SharedTask>(`${root(s)}/shared-v2-items/${encodeURIComponent(attempt)}/reassignments`,{expected_revision,device_id,reason},key),
  usageOptions:(s?:string)=>one<UsageOptions>(s?`${root(s)}/usage-options`:'/api/v1/platform/mobile-execution/usage-options'),
  usages:(s?:string)=>one<UsageList>(s?`${root(s)}/usages`:'/api/v1/platform/mobile-execution/usage-requests'),
  usagePreview:(s:string,input:UsageInput,key:string)=>post<UsagePreview>(`${root(s)}/usage-previews`,input,key),
  usageConfirm:(s:string,input:{preview_id:string;digest:string},key:string)=>post<UsageList>(`${root(s)}/usages`,input,key),
  usageAction:(s:string,id:string,operation:UsageOperation,input:{expected_revision:number;reason:string},key:string)=>post<UsageList>(`${root(s)}/usages/${encodeURIComponent(id)}/${operation}`,input,key),
  usageIssueUpdate:(s:string,id:string,issue:string,input:{expected_revision:number;state:'investigating'|'resolved'|'declined';response:string},key:string)=>post<UsageList>(`${root(s)}/usages/${encodeURIComponent(id)}/issues/${encodeURIComponent(issue)}/responses`,input,key),
  usageIssue:(s:string,id:string,description:string,key:string)=>post<{id:string;state:string}>(`${root(s)}/usages/${encodeURIComponent(id)}/issues`,{description},key),
  taskPreview:(s:string,input:UsageTaskInput,key:string)=>post<{id:string;draft:SharedDraft;preview:SharedPreview}>(`${root(s)}/usage-task-previews`,input,key),
 }
}

export const SHARED_DELEGATION_SCHEMA='shared_mobile_device_delegation/1' as const
export interface SharedDelegationScope {schema:typeof SHARED_DELEGATION_SCHEMA;platform_id:string;device_id:string;credential_generation:number;package_name:'com.tencent.wework';capability:'group_text';resource_policy:'platform_managed_school_targets';background_collection:true;automatic_inspection:true;text_input:true;single_use_send:true}
export interface SharedDelegationDecisionInput {delegation_id:string;scope_digest:string;expected_revision:number;accepted:boolean}

export type FlowV3=Omit<import('../flows').FlowV2,'schema_version'|'engine_version'|'protocol_version'|'launchers'> & {schema_version:3;engine_version:'otter-ui/3';protocol_version:9;title_rule:'exact'|'trailing_member_count';launcher_paths:Array<{kind:'search'|'all_apps';opener:import('../flows').UiSelector;search_input:import('../flows').UiSelector|null;application_row:import('../flows').UiSelector;max_scrolls:number}>}
export interface UsageTarget {id:string|null;name:string;search_text:string}
export interface UsageInput {device_ids:string[];targets:UsageTarget[];max_calls:number;expires_at:string;auto_targets:boolean;renewal_of?:string;expected_revision?:number}
export interface UsageOptions {sending_status?:SendingStatus;business_scheme?:BusinessScheme|null;id:string;schools:Array<{id:string;name:string}>;devices:Array<{id:string;name:string;revision:number;online:boolean}>;targets:Array<UsageTarget & {id:string;revision:number}>;scheme:{id:string|null;timezone:string;allowed_windows:Array<{weekdays:number[];start_minute:number;end_minute:number}>;daily_max_calls:number;interval_seconds:number}|null}
export interface UsageLocator {device_id:string;device_name:string;target_id:string|null;target_index:number;target_name:string;search_text:string;search_source:'default'|'override';binding_locator_revision:number|null;target_locator_revision:number|null}
export interface UsagePreview {id:string;digest:string;body:UsageInput & {scheme:UsageOptions['scheme'];locators?:UsageLocator[]}}
export type UsageOperation='approve'|'decline'|'pause'|'resume'|'revoke'
export interface MobileUsage {readiness?:Array<{device_name:string;target_name:string;state:string;ready:boolean;messages:string[]}>;id:string;budget_root:string;supersedes:string|null;created_at:string;school_id:string;school_name:string;revision:number;approval:'pending'|'approved'|'declined';state:'active'|'paused'|'revoked';body:UsageInput & {locators?:UsageLocator[]};expires_at:string;confirmed_until:string;delivery:'available'|'waiting';progress:string;events:Array<{id:string;action:string;created_at:string}>;issues:Array<{id:string;description:string;state:string;revision:number;response:string}>}
export interface UsageList {id:string;items:MobileUsage[]}
export interface UsageTaskInput {draft_id?:string;expected_revision?:number;target_ids:string[];message:string;between_targets_seconds:number;selections?:Array<{target_id:string;device_id:string}>}

export interface SendingStatus {platform:'default'|'unconfigured'|'disabled'|'draft_only'|'preview_only'|'enabled';can_confirm:boolean;owner:string;message:string}
export interface BusinessSchemeInput {expected_revision:number;enabled:boolean;confirmed:boolean;timezone:string;allowed_windows:SharedPolicyInput['allowed_windows'];limits:SharedPolicyInput['limits'];reason:string}
export type BusinessScheme=SharedPolicyInput & {id:string|null;revision:number}

export interface SharedTaskInboxItem { task_id:string;item_id:string;attempt_id:string;assignment_revision:1;school_id:string;school_name:string;target_id:string;target_name:string;search_text:string;device_id:string;device_name:string;position:number;total:number;assigned_total:number;assigned_at:string;confirmed_until:string;attempt_state:SharedAttemptState;current:boolean;task_stopped:boolean;confirmation_expired:boolean;next_eligible_at:string|null;execution_status:SharedExecutionStatus }
export interface SharedTaskInbox { id:string;device_id:string;credential_generation:number;queue_revision:number;observed_at:string;items:SharedTaskInboxItem[];page:{has_more:boolean;next_cursor:string|null} }
export interface SharedTaskReceiptInput {receipt_id:string;queue_revision:number;items:Array<{attempt_id:string;assignment_revision:1}>}
export interface SharedTaskReceipt {id:string;accepted:Array<{attempt_id:string;received_at:string}>;rejected:Array<{attempt_id:string;code:string}>}
export function createSharedTaskInboxClient(client:Pick<Client,'requestResource'>){return {
 inbox:(limit=50,cursor?:string)=>client.requestResource<SharedTaskInbox>(`/api/v9/device-execution/shared-task-inbox?limit=${limit}${cursor?`&cursor=${encodeURIComponent(cursor)}`:''}`),
 receipt:(input:SharedTaskReceiptInput,key:string)=>client.requestResource<SharedTaskReceipt>('/api/v9/device-execution/shared-task-receipts','POST',input,undefined,{'Idempotency-Key':key})
}}

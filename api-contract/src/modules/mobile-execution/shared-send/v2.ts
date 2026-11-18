import type {MobileClient} from '../client-kind'
import type {ChannelObservation} from '../../execution-evidence'
export {executionEvidenceLabels} from '../../execution-evidence'
import type { Client } from '../../../http/client'
export const SHARED_SEND_SCHEMA = 'shared_mobile_send/3' as const
export interface SharedDraftInput { client_kind:MobileClient; target_ids:string[]; message:string; content_type:'text/plain'; expected_revision?:number;between_targets_seconds:number }
export interface SharedDraft extends SharedDraftInput {business_unit_id:string|null; targets:Array<{id:string;name:string}>;id:string; intent_id:string; revision:number; content_digest:string }
/** UI adaptation and local configuration are owned exclusively by the phone. */
export type DeviceExecutionIntent = {contract:'device_owned/2'} & ({client_kind:'wecom';capability:'wecom.group_text'}|{client_kind:'wechat';capability:'wechat.group_text'})
export interface GroupTarget {kind:'group';search_query:string;match_policy:'exact_unique';identity_constraints:[]}
export interface SharedCandidate {
 client_kind:MobileClient;
 target_id:string; target_name:string;device_name:string;device_id:string; binding_id:string; binding_locator_revision:number;
 target_locator_revision:number|null; target:GroupTarget;
 credential_generation:number;scope_id?:string;confirmable?:boolean;execution:DeviceExecutionIntent;
}
export interface SharedPlan extends SharedCandidate {schema:typeof SHARED_SEND_SCHEMA;intent_id:string;item_id:string;attempt_id:string;message:string;content_digest:string;selection_policy:'exact_unique';between_targets_seconds:number;policy_id:string}
export interface SharedConfigurationIssue {code:string;path:string}
export interface SharedConfigurationStatus {capability:'wecom.group_text';available:boolean;configuration_id:string|null;version:string|null;flow_digest:string|null;issue:SharedConfigurationIssue|null;source:'platform_runtime_configuration';observed_at:string}
export interface SharedPreview { created_at?:string;id:string;digest:string;expires_at:string;plan:{client_kind:MobileClient;business_unit_id:string|null;scheme_snapshot:BusinessScheme;configuration?:SharedConfigurationStatus;sending_status?:SendingStatus;schema:typeof SHARED_SEND_SCHEMA;draft_id:string;draft_revision:number;intent_id:string;message:string;content_digest:string;policy_id:string|null;items:Array<{position:number;target_id:string;target:{id:string;client_kind:MobileClient;name:string;search_text:string;locator_revision:number};selected:SharedCandidate|null;candidates:SharedCandidate[];blocker:string|null;readiness?:SharedReadiness[]}>} }
export type SharedAttemptState='queued'|'leased'|'prepared'|'invocation_issued'|'observed_sent'|'unknown'|'failed_before_call'|'invalidated'|'cancelled'|'verified_not_sent'|'manually_confirmed_sent'
export interface SharedExecutionCondition {waiting_since?:string|null;reason_changed_at?:string|null;code:string;category:string;affected_action:string;scope_type:string;scope_id:string;owner:string;message:string;automatic:boolean;next_action:string;observed_at:string;valid_until:string|null;retry_at:string|null}
export interface SharedExecutionStatus {phase:'waiting'|'ready'|'executing'|'completed'|'blocked'|'unknown';observed_at:string;conditions:SharedExecutionCondition[];device_state_observed_at:string|null}
export interface SharedAttempt {evidence?:ChannelObservation|null;invalidated_at?:string|null;inbox_received_at?:string|null;execution_id?:string|null;execution_accepted_at?:string|null;result_received_at?:string|null;device_started_at?:string|null;device_finished_at?:string|null;execution_status?:SharedExecutionStatus;invalidation_reason?:string|null;result_reason?:string|null;id:string;state:SharedAttemptState;revision:number;plan:SharedPlan;plan_digest:string;attempt_no:number;current:boolean}
export interface SharedDispatchAssignment {id:string;old_attempt_id:string;new_attempt_id:string;actor_id:string;source:'manual'|'automatic';reason:string;created_at:string;from_device_name:string|null;to_device_name:string|null}
export interface SharedDispatchSnapshot {item_id:string;state:'scheduling'|'assigned'|'waiting_resources'|'executing'|'completed'|'needs_review'|'stopped'|'confirmation_expired'|'dispatch_failed';reason:string;revision:number;max_rounds:number;completed_rounds:number;finished_at:string|null;next_retry_at:string|null;updated_at:string;last_failed_attempt_id:string|null;candidates:SharedCandidate[];assignments:SharedDispatchAssignment[]}
export type SharedTaskStatus = 'dispatching'|'completed'|'stopped'|'dispatch_failed'|'confirmation_expired'|'needs_review'
export interface SharedTask {client_kind:MobileClient;business_unit_id:string|null;scheme_snapshot:BusinessScheme;status?:SharedTaskStatus;id:string;intent_id:string;revision:number;stopped:boolean;created_at:string;confirmed_until?:string;next_eligible_at?:string;items?:Array<{id:string;target_id:string;position:number;dispatch?:SharedDispatchSnapshot;attempts:SharedAttempt[]}>}
export interface SharedReadiness {target?:GroupTarget|null;online?:boolean;environment?:DeviceEnvironmentObservation|null;execution_conditions?:Array<{code:string;message:string;owner:string;automatic:boolean;action?:string}>;device_name:string;target_name:string;search_text:string;inspection_state:'waiting_prerequisites'|'not_scheduled'|'queued'|'running'|'passed'|'expired'|'failed'|'stale'|'retry_wait'|'retry_exhausted';retry_at:string|null;retry_remaining:number;binding_id:string;device_id:string;target_id:string;state:string;plan_selectable:boolean;confirmable:boolean;execution_ready:boolean;ready:boolean;automatic:boolean;blockers:Array<{code:string;message:string;owner:string;automatic:boolean}>;inspection:{id:string;state:string;expires_at:string}|null}
export interface SharedOverview {configuration?:SharedConfigurationStatus;sending_status?:SendingStatus;id:string;tasks:SharedTask[];drafts:SharedDraft[];readiness:SharedReadiness[];pool:Array<{id:string;name:string}>;inspections:Array<{id:string;binding_id:string;state:string;expires_at:string}>}
export interface SharedLimit {interval_seconds:number;window_seconds:number;max_calls:number;daily_max_calls:number}
interface ResolvedScheme {expected_revision:number;mode:'disabled'|'enabled';confirmation_seconds:number;inspection_seconds:number;retry_seconds:number;retry_limit:number;timezone:string;allowed_windows:Array<{weekdays:number[];start_minute:number;end_minute:number}>;limits:{total:SharedLimit;device:SharedLimit;target:SharedLimit};reason:string}
export type ObservedFact='yes'|'no'|'unknown'
export interface DeviceEnvironmentObservation {schema:'device_environment/2';observation_id:string;observed_at:string;interactive:ObservedFact;keyguard_locked:ObservedFact;device_locked:ObservedFact;keyguard_secure:ObservedFact;accessibility:ObservedFact;notification:ObservedFact;applications:Record<MobileClient,'logged_in'|'login_required'|'unknown'>}
export function createSharedSendV2Client(client:Pick<Client,'requestResource'>) {
 const root=()=>`/api/v1/mobile-execution`
 const one=client.requestResource
 const post=<T>(path:string,input:unknown,key:string,method='POST')=>one<T>(path,method,input,undefined,{'Idempotency-Key':key})
 return {
  businessScheme:(unit?:string)=>one<BusinessScheme>(`${root()}/business-schemes/${unit?`units/${encodeURIComponent(unit)}`:'default'}`),
  saveBusinessScheme:(input:BusinessSchemeInput,key:string,unit?:string)=>post<BusinessScheme>(`${root()}/business-schemes/${unit?`units/${encodeURIComponent(unit)}`:'default'}/versions`,input,key),
  inheritBusinessScheme:(unit:string,input:{expected_revision:number;confirmed:boolean;reason:string},key:string)=>post<BusinessScheme>(`${root()}/business-schemes/units/${encodeURIComponent(unit)}/inheritances`,input,key),
  overview:()=>one<SharedOverview>(`${root()}/shared-v2-tasks`),
  task:(id:string)=>one<SharedTask>(`${root()}/shared-v2-tasks/${encodeURIComponent(id)}`),
  confirm:(p:SharedPreview,key:string)=>post<SharedTask>(`${root()}/shared-v2-tasks`,{preview_id:p.id,preview_digest:p.digest,confirmed:true},key),
  stop:(id:string,expected_revision:number,reason:string,key:string)=>post<SharedTask>(`${root()}/shared-v2-tasks/${encodeURIComponent(id)}/stops`,{expected_revision,reason},key),
  review:(attempt:string,expected_revision:number,decision:'manually_confirmed_sent'|'verified_not_sent'|'still_unknown',evidence:string,key:string)=>post<{id:string}>(`${root()}/shared-v2-items/${encodeURIComponent(attempt)}/reviews`,{expected_revision,decision,evidence},key),
  inspection:(target_id:string,device_id:string,key:string)=>post<{id:string;state:string;readiness:SharedReadiness}>(`${root()}/shared-v2-inspections`,{target_id,device_id},key),
  reassign:(attempt:string,expected_revision:number,device_id:string,reason:string,key:string)=>post<SharedTask>(`${root()}/shared-v2-items/${encodeURIComponent(attempt)}/reassignments`,{expected_revision,device_id,reason},key),
  taskPreview:(input:TaskPreviewInput,key:string)=>post<{id:string;draft:SharedDraft;preview:SharedPreview}>(`${root()}/shared-v2-task-previews`,input,key),
 }
}


export type FlowV3=Omit<import('../flows').FlowV2,'schema_version'|'engine_version'|'protocol_version'|'launchers'> & {schema_version:3;engine_version:'otter-ui/3';protocol_version:9;title_rule:'exact'|'trailing_member_count';launcher_paths:Array<{kind:'search'|'all_apps';opener:import('../flows').UiSelector;search_input:import('../flows').UiSelector|null;application_row:import('../flows').UiSelector;max_scrolls:number}>}
export interface TaskPreviewInput {draft_id?:string;expected_revision?:number;business_unit_id:string|null;target_ids:string[];message:string;between_targets_seconds:number;selections?:Array<{target_id:string;device_id:string}>}

export interface SendingStatus {platform:'default'|'disabled'|'enabled';can_confirm:boolean;owner:string;message:string}
export interface BusinessSchemeInput {expected_revision:number;enabled:boolean;confirmed:boolean;timezone:string;allowed_windows:ResolvedScheme['allowed_windows'];limits:ResolvedScheme['limits'];reason:string}
// HTTP built-ins use builtin:<scope> resource IDs; frozen business snapshots keep id:null.
export type BusinessScheme=ResolvedScheme & {id:string|null;revision:number;scope_revision:number;source:'builtin'|'default'|'unit';business_unit_id:string|null;builtin_version?:string}

export interface SharedTaskInboxItem {client_kind:MobileClient; task_id:string;item_id:string;attempt_id:string;assignment_revision:1;target_id:string;target_name:string;search_text:string;device_id:string;device_name:string;position:number;total:number;assigned_total:number;assigned_at:string;confirmed_until:string;attempt_state:SharedAttemptState;current:boolean;task_stopped:boolean;confirmation_expired:boolean;next_eligible_at:string|null;execution_status:SharedExecutionStatus }
export interface SharedTaskInbox { id:string;device_id:string;credential_generation:number;queue_revision:number;observed_at:string;items:SharedTaskInboxItem[];page:{has_more:boolean;next_cursor:string|null} }
export interface SharedTaskReceiptInput {receipt_id:string;queue_revision:number;items:Array<{attempt_id:string;assignment_revision:1}>}
export interface SharedTaskReceipt {id:string;accepted:Array<{attempt_id:string;received_at:string}>;rejected:Array<{attempt_id:string;code:string}>}
export function createSharedTaskInboxClient(client:Pick<Client,'requestResource'>){return {
 inbox:(limit=50,cursor?:string)=>client.requestResource<SharedTaskInbox>(`/api/v10/device-execution/task-inbox?limit=${limit}${cursor?`&cursor=${encodeURIComponent(cursor)}`:''}`),
 receipt:(input:SharedTaskReceiptInput,key:string)=>client.requestResource<SharedTaskReceipt>('/api/v10/device-execution/display-receipts','POST',input,undefined,{'Idempotency-Key':key})
}}

export interface InspectionRequest {runtime_epoch:string;inspection_id:string}
export interface TaskDelivery {id:string;schema:'task_delivery/1';state:'granted'|'idle'|'waiting'|'superseded';reason:string|null;verification_key:string;execution:ExecutionGrant|null}
interface ExecutionGrantBase {
 id:string;execution_id:string;capability_id:string;schema:'durable_execution/1';protocol_version:10;
 state:'granted';local_budget_ms:number;device_id:string;credential_generation:number;plan_digest:string;
 timing:{server_time:string;not_before:string;execute_before:string};result_credential:string;signature:string;key_id:string;
}
export type ExecutionGrant = ExecutionGrantBase & (
 {kind:'send';max_calls:1;attempt_id:string;task_id:string;target_id:string;assignment_revision:number;runtime_epoch:string;plan:SharedPlan & {target:GroupTarget;execution:DeviceExecutionIntent;search_text?:never;flow?:never;flow_digest?:never};content_digest:string}
 | {kind:'inspection';max_calls:0;inspection_id:string;plan:SharedCandidate & {target:GroupTarget;execution:DeviceExecutionIntent;search_text?:never;flow?:never;flow_digest?:never}}
)
export interface ExecutionCompletion {
 /** Device timestamps are optional, untrusted observations; they never drive platform deadlines. */
 device_started_at?:string;device_finished_at?:string;
 result_id:string;execution_id:string;result_credential:string;outcome:'observed_sent'|'not_called'|'failed'|'unknown'|'inspection_passed'|'inspection_failed';
 reason:string;evidence:string;consumed:boolean;calling_marked:boolean;
}
export interface ExecutionCompletionReceipt {id:string;result_id:string;execution_id:string;body_digest:string;recorded:true}
export function durableExecutionApi(client:Client){
 const post=<T>(path:string,body:unknown,key:string)=>client.requestResource<T>(`/api/v10/device-execution/${path}`,'POST',body,undefined,{'Idempotency-Key':key});
 return {
  observeEnvironment:(body:DeviceEnvironmentObservation,key:string)=>post<{id:string;recorded:true}>('environment-observations',body,key),
  inspect:(body:InspectionRequest,key:string)=>post<TaskDelivery>('inspection-executions',body,key),
  complete:(body:ExecutionCompletion)=>post<ExecutionCompletionReceipt>('execution-completions',body,body.result_id),
 };
}

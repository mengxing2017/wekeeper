import type { Client } from '../../../http/client'
import type { Json } from '../../business/types'
export const SHARED_SEND_SCHEMA = 'shared_mobile_send/2' as const
export interface SharedDraftInput { target_ids:string[]; message:string; content_type:'text/plain'; expected_revision?:number;stage:"test_send"|"production_allowlist";between_targets_seconds:number }
export interface SharedDraft extends SharedDraftInput { targets:Array<{id:string;name:string}>;id:string; school_id:string; intent_id:string; revision:number; content_digest:string }
export interface SharedCandidate {
 target_id:string; target_name:string;device_name:string;device_id:string; binding_id:string; binding_locator_revision:number;
 target_locator_revision:number|null; search_text:string; search_source:'default'|'override';
 access_revision:number|null;credential_generation:number; delegation_id:string|null; release_id:string|null; flow_digest:string|null;
 stage?:'test_send'|'production_allowlist';scope_id?:string;confirmable?:boolean;flow:Record<string,Json>|null;
}
export interface SharedPlan extends SharedCandidate {schema:typeof SHARED_SEND_SCHEMA;school_id:string;intent_id:string;item_id:string;attempt_id:string;message:string;content_digest:string;selection_policy:'first_group_result';stage:'test_send'|'production_allowlist';between_targets_seconds:number;policy_id:string}
export interface SharedPreview { id:string;digest:string;expires_at:string;plan:{schema:typeof SHARED_SEND_SCHEMA;school_id:string;draft_id:string;draft_revision:number;intent_id:string;message:string;content_digest:string;policy_id:string|null;items:Array<{position:number;target_id:string;target:{id:string;name:string;search_text:string;locator_revision:number};selected:SharedCandidate|null;candidates:SharedCandidate[];blocker:string|null;readiness?:SharedReadiness[]}>} }
export type SharedAttemptState='queued'|'leased'|'prepared'|'invocation_issued'|'observed_sent'|'unknown'|'failed_before_call'|'invalidated'|'cancelled'|'verified_not_sent'|'manually_confirmed_sent'
export interface SharedAttempt {id:string;state:SharedAttemptState;revision:number;plan:SharedPlan;plan_digest:string;attempt_no:number;current:boolean}
export interface SharedTask {id:string;school_id:string;intent_id:string;revision:number;stopped:boolean;created_at:string;confirmed_until?:string;cursor_position?:number;next_eligible_at?:string;items?:Array<{id:string;target_id:string;position:number;attempts:SharedAttempt[]}>}
export interface SharedReadiness {device_name:string;target_name:string;search_text:string;inspection_state:'waiting_prerequisites'|'not_scheduled'|'queued'|'running'|'passed'|'expired'|'failed'|'stale'|'retry_wait'|'retry_exhausted';retry_at:string|null;retry_remaining:number;binding_id:string;device_id:string;target_id:string;state:string;plan_selectable:boolean;confirmable:boolean;execution_ready:boolean;confirmable_stages:string[];ready:boolean;automatic:boolean;blockers:Array<{code:string;message:string;owner:string;automatic:boolean}>;inspection:{id:string;state:string;expires_at:string}|null}
export interface SharedOverview {id:string;tasks:SharedTask[];drafts:SharedDraft[];readiness:SharedReadiness[];pool:Array<{id:string;name:string;access_revision:number}>;inspections:Array<{id:string;binding_id:string;state:string;expires_at:string}>}
export interface SharedLimit {interval_seconds:number;window_seconds:number;max_calls:number;daily_max_calls:number}
export interface SharedPolicyInput {expected_revision:number;mode:'disabled'|'draft_only'|'preview_only'|'enabled';allowed_stages:Array<'test_send'|'production_allowlist'>;confirmation_seconds:number;inspection_seconds:number;retry_seconds:number;retry_limit:number;timezone:string;allowed_windows:Array<{weekdays:number[];start_minute:number;end_minute:number}>;limits:{global:SharedLimit;school:SharedLimit;device:SharedLimit;target:SharedLimit};reason:string}
export interface SharedRuntimeReport {schema:typeof SHARED_SEND_SCHEMA;runtime_epoch:string;ui_ready:boolean;paused:boolean;quiescent:boolean;ui_context_epoch:string;apk_sha256:string;conditions:Record<string,boolean>}
export interface SharedResultEvent {result_credential:string|null;schema:typeof SHARED_SEND_SCHEMA;event_id:string;claim_id:string;attempt_id:string;invocation_id:string|null;credential_generation:number;lease_fence:number;plan_digest:string;content_digest:string;outcome:'observed_sent'|'unknown'|'failed_before_call';reason:string;consumed:boolean;calling_marked:boolean;evidence:string}
export interface SharedReceipt {id:string;event_id:string;claim_id:string;attempt_id:string;invocation_id:string|null;device_id:string;credential_generation:number;lease_fence:number;plan_digest:string;content_digest:string;body_hash:string;disposition:'observed_sent'|'unknown'|'failed_before_call'|'archived'}
export function createSharedSendV2Client(client:Pick<Client,'requestResource'>) {
 const root=(s:string)=>`/api/v1/schools/${encodeURIComponent(s)}/mobile-execution`
 const one=client.requestResource
 const post=<T>(path:string,input:unknown,key:string,method='POST')=>one<T>(path,method,input,undefined,{'Idempotency-Key':key})
 return {
  bind:(s:string,input:{target_id:string;device_id:string;expected_access_revision:number;search_text_override:string|null},key:string)=>post<{id:string}>(`${root(s)}/shared-v2-bindings`,input,key),
  unbind:(s:string,id:string,expected_revision:number,key:string)=>post<{id:string}>(`${root(s)}/shared-v2-bindings/${encodeURIComponent(id)}/revocations`,{expected_revision,reason:'学校管理员停用本校绑定'},key),
  access:(input:{school_id:string;device_id:string;expected_revision:number;enabled:boolean},key:string)=>post<{id:string;revision:number}>('/api/v1/platform/mobile-execution/shared-v2-device-access',input,key),
  overview:(s:string)=>one<SharedOverview>(`${root(s)}/shared-v2-tasks`),
  task:(s:string,id:string)=>one<SharedTask>(`${root(s)}/shared-v2-tasks/${encodeURIComponent(id)}`),
  draft:(s:string,input:SharedDraftInput,key:string,id?:string)=>post<SharedDraft>(`${root(s)}/shared-v2-task-drafts${id?`/${encodeURIComponent(id)}`:''}`,input,key,id?'PATCH':'POST'),
  preview:(s:string,draft_id:string,expected_revision:number,selections:Array<{target_id:string;device_id:string}>,key:string)=>post<SharedPreview>(`${root(s)}/shared-v2-task-previews`,{draft_id,expected_revision,selections},key),
  confirm:(s:string,p:SharedPreview,key:string)=>post<SharedTask>(`${root(s)}/shared-v2-tasks`,{preview_id:p.id,preview_digest:p.digest,confirmed:true},key),
  stop:(s:string,id:string,expected_revision:number,reason:string,key:string)=>post<SharedTask>(`${root(s)}/shared-v2-tasks/${encodeURIComponent(id)}/stops`,{expected_revision,reason},key),
  review:(s:string,attempt:string,expected_revision:number,decision:'manually_confirmed_sent'|'verified_not_sent'|'still_unknown',evidence:string,key:string)=>post<{id:string}>(`${root(s)}/shared-v2-items/${encodeURIComponent(attempt)}/reviews`,{expected_revision,decision,evidence},key),
  inspection:(s:string,target_id:string,device_id:string,key:string)=>post<{id:string;state:string;readiness:SharedReadiness}>(`${root(s)}/shared-v2-inspections`,{target_id,device_id},key),
  reassign:(s:string,attempt:string,expected_revision:number,device_id:string,reason:string,key:string)=>post<SharedTask>(`${root(s)}/shared-v2-items/${encodeURIComponent(attempt)}/reassignments`,{expected_revision,device_id,reason},key),
  platform:(page=1)=>one<ManagementOverview & {id:string;schools:Array<{id:string;name:string}>;access:Array<{id:string;school_id:string;device_id:string;device_name:string;school_name:string;revision:number;enabled:boolean}>;policy:(Omit<SharedPolicyInput,'expected_revision'>&{id:string;revision:number})|null;releases:Array<{id:string;name:string;version:string}>;devices:Array<{device_id:string;name:string;enabled:boolean}>}>( `/api/v1/platform/mobile-execution/shared-v2-readiness?page=${page}`),
  support:(s:string,device_id:string,reason:string,key:string)=>post<{id:string}>(`${root(s)}/shared-v2-support`,{device_id,school_id:s,reason},key),
  manage:(input:ManagementCommand,key:string)=>post<{id:string;revision?:number}>('/api/v1/platform/mobile-execution/shared-v2-management',input,key),
  upload:(content:ArrayBuffer,kind:'apk'|'evidence',name:string,key:string)=>one<{id:string;sha256:string}>('/api/v1/platform/mobile-execution/shared-v2-artifacts','POST',content,undefined,{'Idempotency-Key':key,'X-Artifact-Kind':kind,'X-Artifact-Name':encodeURIComponent(name)}),
  artifactUrl:(id:string)=>`/api/v1/platform/mobile-execution/shared-v2-artifacts/${encodeURIComponent(id)}/content`,
  policy:(input:SharedPolicyInput,key:string)=>post<{id:string;revision:number}>('/api/v1/platform/mobile-execution/shared-v2-send-policies',input,key),
 }
}

export const SHARED_DELEGATION_SCHEMA='shared_mobile_device_delegation/1' as const
export interface SharedDelegationScope {schema:typeof SHARED_DELEGATION_SCHEMA;platform_id:string;device_id:string;credential_generation:number;package_name:'com.tencent.wework';capability:'group_text';resource_policy:'platform_managed_school_targets';background_collection:true;automatic_inspection:true;text_input:true;single_use_send:true}
export interface SharedDelegationDecisionInput {delegation_id:string;scope_digest:string;expected_revision:number;accepted:boolean}

export type FlowV3=Omit<import('../flows').FlowV2,'schema_version'|'engine_version'|'protocol_version'|'launchers'> & {schema_version:3;engine_version:'otter-ui/3';protocol_version:9;title_rule:'exact'|'trailing_member_count';launcher_paths:Array<{kind:'search'|'all_apps';opener:import('../flows').UiSelector;search_input:import('../flows').UiSelector|null;application_row:import('../flows').UiSelector;max_scrolls:number}>}
export type ManagedRecord={id:string;name?:string;created_at?:string;flow_id?:string;release_id?:string;flow?:FlowV3;stage?:string;device_id?:string;school_id?:string;target_id?:string;state?:string;code?:string;revision?:number;min_interval_ms?:number;elapsed_ms?:number;kind?:string;sha256?:string;device_ids?:string[];target_ids?:string[];auto_targets?:boolean;expires_at?:string;max_calls?:number;response?:string;reason?:string;result?:Record<string,Json>;launch_path?:string;device_name?:string;school_name?:string;target_name?:string}
export interface ManagementOverview {page?:{number:number;size:number;has_more:boolean};counts?:Record<string,number>;flows:ManagedRecord[];baselines:ManagedRecord[];samples:ManagedRecord[];releases:ManagedRecord[];acceptances:ManagedRecord[];scopes:ManagedRecord[];revocations:Array<{grant_id:string}>;validations:ManagedRecord[];support:ManagedRecord[];withdrawals:Array<{release_id:string}>;diagnostics:ManagedRecord[];artifacts:ManagedRecord[];bindings:ManagedRecord[];targets:ManagedRecord[];activation:{release_id:string;revision:number}|null}
export type ManagementCommand=
 |{action:'flows';input:{name:string;flow:FlowV3}}
 |{action:'human-samples';input:{flow_id:string;device_id:string}}
 |{action:'baselines';input:{flow_id:string;sample_ids:string[];evidence_ids:string[]}}
 |{action:'releases';input:{name:string;version:string;flow_id:string;baseline_id:string;evidence_ids:string[]}}
 |{action:'activations';input:{release_id:string;expected_revision:number;reason:string}}
 |{action:'withdrawals';input:{release_id:string;reason:string}}
 |{action:'validations';input:{flow_id:string;binding_id:string;launch_path:string;apk_id:string}}
 |{action:'acceptances';input:{release_id:string;stage:string;apk_id:string;evidence_ids:string[];validation_ids:string[];cases:Record<string,boolean>;soak_hours:number;soak_passed:boolean;historical_debt_closed:boolean;reason:string}}
 |{action:'scopes';input:{acceptance_id:string;school_id:string;device_ids:string[];target_ids:string[];auto_targets:boolean;max_calls:number;expires_at:string;expected_access:Record<string,number>;reason:string}}
 |{action:'revocations';input:{grant_id:string;reason:string}}
 |{action:'support';input:{device_id:string;school_id:string|null;reason:string}}
 |{action:'support-updates';input:{id:string;expected_revision:number;state:string;response:string;release_id:string|null}}

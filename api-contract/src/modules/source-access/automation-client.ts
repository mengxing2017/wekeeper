import type {Client} from '../../http/client'
export type AutomationState='queued'|'scheduled'|'running'|'succeeded'|'partially_succeeded'|'failed'|'stopped'|'unknown'
export interface AutomationCounts {total:number;pending:number;sent:number;failed:number;unknown:number;stopped:number;cancelled:number}
export interface AutomationTask {id:string;decision_id:string;source_id:string;route_id:string;unit_id:string;channel:'mobile'|'wecom';workspace_id:string;route_revision:number;event_id:string;external_content_id:string;state:AutomationState;counts:AutomationCounts;stopped:boolean;stop_reason:string|null;revision:number;created_at:string;has_unknown:boolean;can_stop:boolean;can_retry:boolean}
export interface AutomationFilter {source_id?:string;unit_id?:string;route_id?:string;workspace_id?:string;channel?:string;state?:string;from?:string;until?:string;page?:number}
export interface AutomationDetail {task:AutomationTask;frozen:{snapshot:{targets:{id:string;name:string}[]};content:string;authorized_by:string;executed_by:string;reconstructed:boolean;version_policy?:VersionDeliveryPolicy};composition:{content:string;selection:Record<string,unknown>;common_check:string;workspace_check:string;final_check:string;generation:number}|null}
export interface AutomationEvent {id:number;kind:string;detail:Record<string,unknown>;actor_id:string|null;executed_by:string;created_at:string}
export interface AutomationTarget {target_id:string;name:string;state:string;reason:string|null;has_unknown:boolean;next_attempt_at:string|null}
export interface AutomationAttempt {id:string;channel:string;channel_task_id:string;generation:number;state:string;attempt_no?:number;current?:boolean;device_id?:string;invocation_id?:string|null;msgid?:string|null;error_code?:string|null;sync_error?:string|null;stop_state?:string;received_at?:string|null;result_received_at?:string|null;result_status?:string|null;grant_id?:string|null}
export interface AutomationControl {expected_revision:number;idempotency_key:string;reason:string;target_ids?:string[]}
export interface VersionDeliveryPolicy {policy:'replace_unsubmitted'|'resend_new_version';revision:number;effective_at:string|null}
export function createSourceAutomationClient({requestResource}:Pick<Client,'requestResource'>){
 const root='/api/v2/admin/source-automation-tasks'
 const path=(id:string)=>`${root}/${encodeURIComponent(id)}`
 return {
 list:(filter:AutomationFilter={})=>requestResource<{items:AutomationTask[];has_more:boolean}>(`${root}?${Object.entries(filter).filter(([,v])=>v!==undefined&&v!=='').map(([k,v])=>`${k}=${encodeURIComponent(String(v))}`).join('&')}`),
 detail:(id:string)=>requestResource<AutomationDetail>(path(id)),
 events:(id:string,after=0)=>requestResource<{items:AutomationEvent[];next_cursor:number|null}>(`${path(id)}/events?after=${after}`),
 targets:(id:string)=>requestResource<{items:AutomationTarget[];excluded:{target_id:string;reason:string}[]}>(`${path(id)}/targets`),
 attempts:(id:string,target:string)=>requestResource<{items:AutomationAttempt[]}>(`${path(id)}/targets/${encodeURIComponent(target)}/attempts`),
 stop:(id:string,input:AutomationControl)=>requestResource<{id:string;accepted:boolean}>(`${path(id)}/stop`,'POST',input),
 retry:(id:string,input:AutomationControl)=>requestResource<{id:string;accepted:boolean}>(`${path(id)}/retry-requests`,'POST',input),
 policy:(route:string)=>requestResource<VersionDeliveryPolicy>(`/api/v2/admin/source-routes/${encodeURIComponent(route)}/version-policy`),
 savePolicy:(route:string,revision:number,policy:VersionDeliveryPolicy['policy'],reason:string)=>requestResource<VersionDeliveryPolicy>(`/api/v2/admin/source-routes/${encodeURIComponent(route)}/version-policy`,'PUT',{revision,policy,reason}),
 }
}
export const automationStateLabels:Record<string,string>={queued:'等待调度',scheduled:'等待计划时间',running:'进行中',succeeded:'全部成功',partially_succeeded:'部分成功',failed:'失败',stopped:'已停止',unknown:'结果不确定',sent:'已发送',dispatching:'正在提交',retry_wait:'等待重试',cancelled:'已取消'}

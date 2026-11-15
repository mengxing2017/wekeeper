import type {Client} from '../../http/client'
export interface Confirmation {preview_id:string;digest:string}
export interface BusinessPreview<T> {id:string;digest:string;expires_at:string;summary:T;rendered?:string;sample?:boolean}
export interface TemplateInput {id?:string|null;revision:number;name:string;body:string;note:string;sample_post_id?:string|null;publish:boolean}
export interface MemberPolicyInput {member_id:string;generation:number;revision:number;enabled:boolean;window_seconds:number;max_calls:number}
export interface UsageInput {member_id:string;account_id:string;generation:number;group_ids:string[]}
export interface RequestInput {reason:string;task_id?:string|null;batch_id?:string|null}
export interface BusinessRequest {id:string;kind:'channel'|'official_usage'|'broadcast_issue';state:string;revision:number;owner_id:string|null;owner_name?:string|null;response:string;created_at:string;updated_at:string;payload:RequestInput & {input?:UsageInput;summary?:{member_name?:string;account_name?:string;group_names?:string[];content?:string;target_count?:number;created_at?:string}};events?:{id:string;state:string;response:string;created_at:string}[]}
export interface RequestDecision {revision:number;state:'investigating'|'waiting_user'|'approved'|'declined'|'resolved'|'revoked';response:string;owner_id?:string|null}
export interface OfficialUsageOptions {id:string;owners:{id:string;name:string}[];generation:number|null;reason:string|null;members:{id:string;name:string;verified:boolean;bound_account_id:string|null;account_name:string|null;policy:Omit<MemberPolicyInput,'member_id'|'generation'>|null;used:number|null;restores_at:string|null}[];accounts:{id:string;name:string;member_id:string|null}[];groups:{id:string;name:string|null;owner_member_id:string;checked_at:string}[]}
export function createProductClient(client:Pick<Client,'requestResource'|'requestCollection'>){
 const {requestResource:r,requestCollection:c}=client
 const root=()=>'/api/v1/wecom-distribution'
 return {
 officialChannelStatus:()=>r<{id:string;configured:boolean;paused:boolean;revision:number;health_state:'available'|'checking'|'paused';health:{checked_at:string|null;last_success_at:string|null;last_failure_at:string|null}|null}>('/api/v1/platform/official-channel-controls'),
 officialChannelControl:(input:{paused:boolean;expected_revision:number;reason:string})=>r<{id:string;paused:boolean;revision:number}>('/api/v1/platform/official-channel-controls','POST',input),
 templates:{preview:(input:TemplateInput)=>r<BusinessPreview<TemplateInput>>(`${root()}/template-previews`,'POST',{...input,id:input.id??undefined}),confirm:(input:Confirmation)=>r<{id:string;published:boolean;version_id:string|null}>(`${root()}/template-publications`,'POST',input)},
 officialUsage:{options:()=>r<OfficialUsageOptions>(`${root()}/official-usage-options`),preview:(input:UsageInput)=>r<BusinessPreview<UsageInput>>(`${root()}/official-usage-previews`,'POST',input),confirm:(input:Confirmation)=>r<{id:string;state:string}>(`${root()}/official-usages`,'POST',input)},
 memberPolicies:{preview:(input:MemberPolicyInput)=>r<BusinessPreview<MemberPolicyInput>>('/api/v1/platform/member-policy-previews','POST',input),confirm:(input:Confirmation)=>r<{id:string;updated:boolean}>('/api/v1/platform/member-policies','POST',input)},
 requests:{list:(page=1)=>c<BusinessRequest>(`${root()}/business-requests?page[number]=${page}&page[size]=30`),channel:(input:RequestInput)=>r<BusinessRequest>(`${root()}/broadcast-access-requests`,'POST',input),issue:(input:RequestInput)=>r<BusinessRequest>(`${root()}/broadcast-issues`,'POST',input),decide:(id:string,input:RequestDecision)=>r<BusinessRequest>(`/api/v1/platform/business-requests/${encodeURIComponent(id)}/decisions`,'POST',input)},
 }
}

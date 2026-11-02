import type {Client} from '../../http/client'
export type ImageRef = {kind:'asset';id:string}
export type ResourceAction = {kind:'none'}|{kind:'internal_page';page:'feed'|'resources'|'legal'|'post';object_id?:string|null}|{kind:'external_link';url:string}|{kind:'phone';number:string}|{kind:'copy_text';text:string}
export interface ResourceInput {id?:string|null;revision:number;kind:'banner'|'qrcode'|'service'|'legal'|'tag'|'blocked_word';title:string;content:string;image_ref:ImageRef|null;action:ResourceAction;active:boolean;sort_order:number;starts_at:string|null;ends_at:string|null}
export interface PublishedResource extends ResourceInput {id:string;image_url:string|null}
export interface Artwork {id:string;image_ref:ImageRef;url?:string;width?:number;height?:number}
export interface Confirmation {preview_id:string;digest:string}
export interface BusinessPreview<T> {id:string;digest:string;expires_at:string;summary:T;rendered?:string;sample?:boolean;requires_approval?:boolean}
export interface TemplateInput {id?:string|null;revision:number;name:string;body:string;note:string;sample_post_id?:string|null;publish:boolean}
export interface MemberPolicyInput {member_id:string;generation:number;revision:number;enabled:boolean;window_seconds:number;max_calls:number}
export interface UsageInput {member_id:string;account_id:string;generation:number;role:'operator'|'admin';group_ids:string[]}
export interface RequestInput {reason:string;task_id?:string|null;batch_id?:string|null}
export interface BusinessRequest {id:string;school_id:string;school_name?:string;kind:'channel'|'official_usage'|'broadcast_issue';state:string;revision:number;owner_id:string|null;owner_name?:string|null;response:string;created_at:string;updated_at:string;payload:RequestInput & {input?:UsageInput;summary?:{member_name?:string;account_name?:string;role?:string;group_names?:string[];content?:string;target_count?:number;created_at?:string}};events?:{id:string;state:string;response:string;created_at:string}[]}
export interface RequestDecision {revision:number;state:'investigating'|'waiting_user'|'approved'|'declined'|'resolved'|'revoked';response:string;owner_id?:string|null}
export interface OfficialUsageOptions {id:string;owners:{id:string;name:string}[];generation:number|null;reason:string|null;members:{id:string;name:string;verified:boolean;bound_account_id:string|null;account_name:string|null;policy:Omit<MemberPolicyInput,'member_id'|'generation'>|null;used:number|null;restores_at:string|null}[];accounts:{id:string;name:string;member_id:string|null;role:string|null}[];groups:{id:string;name:string|null;owner_member_id:string;school_id:string|null;checked_at:string}[]}
export function createProductClient(client:Pick<Client,'requestResource'|'requestCollection'>){
 const {requestResource:r,requestCollection:c}=client
 const root=(s?:string)=>s?`/api/v1/schools/${encodeURIComponent(s)}`:'/api/v1/platform'
 return {
 resources:{
 list:(school?:string)=>r<PublishedResource[]>(`${root(school)}/${school?'published-resources':'resources'}`),
 preview:(school:string|undefined,input:ResourceInput)=>r<BusinessPreview<ResourceInput>>(`${root(school)}/resource-previews`,'POST',{...input,id:input.id??undefined}),
 confirm:(school:string|undefined,input:Confirmation)=>r<{id:string;published:boolean}>(`${root(school)}/resource-publications`,'POST',input),
 upload:(school:string|undefined,bytes:ArrayBuffer)=>r<Artwork>(`${root(school)}/resource-artwork`,'POST',bytes),
 artwork:(school?:string)=>r<Artwork[]>(`${root(school)}/resource-artwork`),
 qr:(school:string|undefined,url:string)=>r<Artwork>(`${root(school)}/resource-qrcodes`,'POST',{url}),
 },
 templates:{preview:(school:string,input:TemplateInput)=>r<BusinessPreview<TemplateInput>>(`${root(school)}/template-previews`,'POST',{...input,id:input.id??undefined}),confirm:(school:string,input:Confirmation)=>r<{id:string;published:boolean;version_id:string|null}>(`${root(school)}/template-publications`,'POST',input)},
 officialUsage:{options:(school?:string)=>r<OfficialUsageOptions>(`${root(school)}/official-usage-options`),preview:(school:string,input:UsageInput)=>r<BusinessPreview<UsageInput>>(`${root(school)}/official-usage-previews`,'POST',input),confirm:(school:string,input:Confirmation)=>r<{id:string;state:string}>(`${root(school)}/official-usages`,'POST',input)},
 memberPolicies:{preview:(input:MemberPolicyInput)=>r<BusinessPreview<MemberPolicyInput>>('/api/v1/platform/member-policy-previews','POST',input),confirm:(input:Confirmation)=>r<{id:string;updated:boolean}>('/api/v1/platform/member-policies','POST',input)},
 requests:{list:(school?:string,page=1)=>c<BusinessRequest>(`${root(school)}/business-requests?page[number]=${page}&page[size]=30`),channel:(school:string,input:RequestInput)=>r<BusinessRequest>(`${root(school)}/broadcast-access-requests`,'POST',input),issue:(school:string,input:RequestInput)=>r<BusinessRequest>(`${root(school)}/broadcast-issues`,'POST',input),decide:(id:string,input:RequestDecision)=>r<BusinessRequest>(`/api/v1/platform/business-requests/${encodeURIComponent(id)}/decisions`,'POST',input)},
 }
}

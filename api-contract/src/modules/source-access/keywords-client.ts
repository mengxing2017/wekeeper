import type {Client} from '../../http/client'
export interface KeywordEntry {word:string;enabled:boolean;normalized?:string}
export interface KeywordScope {id:string;kind:'global'|'source'|'unit'|'route'|'workspace';object_id:string;channel:'mobile'|'wecom'|null;label:string;revision:number;version_id:string;can_edit:boolean;recheck_complete:boolean}
export interface KeywordVersion {id:string;scope_id:string;revision:number;entries:KeywordEntry[];actor_id:string|null;reason:string;created_at:string;legacy_revision:number|null}
export interface KeywordPublication {expected_revision:number;reason:string;entries:KeywordEntry[]}
export interface KeywordRule {id:string;scope_id:string;version_id:string;word:string}
export interface KeywordContext {source_id:string;route_id:string;unit_id:string|null;channel:string|null;workspace_id:string|null;versions:{scope_id:string;revision:number;version_id:string}[]}
export interface KeywordCheck {id:string;receipt_id:string;stage:'original_common'|'original_workspace'|'final';payload_fingerprint:string;context:KeywordContext;effective_fingerprint:string;input_digest:string;normalized_digest:string;algorithm:string;unicode_version:string;outcome:'passed'|'blocked';total_hits:number;truncated:boolean;created_at:string}
export interface KeywordHits {items:KeywordRule[];has_more:boolean;samples:{rule_id:string;field:string;start:number;end:number}[];total_hits:number;truncated:boolean}
const scopes='/api/v2/admin/keyword-rule-scopes'
const checks=(route:string,receipt:string)=>`/api/v2/admin/source-routes/${encodeURIComponent(route)}/receipts/${encodeURIComponent(receipt)}/checks`
export function createKeywordRulesClient({requestResource}:Pick<Client,'requestResource'>){return {
 scopes:()=>requestResource<{items:KeywordScope[]}>(scopes),
 versions:(scope:string,page=1)=>requestResource<{items:KeywordVersion[];has_more:boolean;algorithm:string;unicode_version:string}>(`${scopes}/${encodeURIComponent(scope)}/versions?page=${page}`),
 publish:(scope:string,input:KeywordPublication)=>requestResource<{id:string;revision:number;version_id:string;batch_id:string}>(`${scopes}/${encodeURIComponent(scope)}/publications`,'POST',input),
 checks:(route:string,receipt:string,page=1)=>requestResource<{items:KeywordCheck[];has_more:boolean}>(`${checks(route,receipt)}?page=${page}`),
 hits:(route:string,receipt:string,check:string,page=1)=>requestResource<KeywordHits>(`${checks(route,receipt)}/${encodeURIComponent(check)}/hits?page=${page}`),
}}

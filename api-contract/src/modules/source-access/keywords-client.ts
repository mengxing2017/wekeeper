import type {Client} from '../../http/client'
export interface KeywordEntry {word:string;enabled:boolean;normalized?:string}
export interface SensitiveWords {revision:number;entries:KeywordEntry[]}
export interface SensitiveWordsUpdate {expected_revision:number;entries:KeywordEntry[]}
export interface KeywordRule {id:string;scope_id:string;version_id:string;word:string}
export interface KeywordContext {source_id:string;route_id:string;unit_id:string|null;channel:string|null;workspace_id:string|null;versions:{scope_id:string;revision:number;version_id:string}[]}
export interface KeywordCheck {id:string;receipt_id:string;stage:'original_common'|'original_workspace'|'final';payload_fingerprint:string;context:KeywordContext;effective_fingerprint:string;input_digest:string;normalized_digest:string;algorithm:string;unicode_version:string;outcome:'passed'|'blocked';total_hits:number;truncated:boolean;created_at:string}
export interface KeywordHits {items:KeywordRule[];has_more:boolean;samples:{rule_id:string;field:string;start:number;end:number}[];total_hits:number;truncated:boolean}
const words='/api/v2/admin/sensitive-words'
const checks=(route:string,receipt:string)=>`/api/v2/admin/source-routes/${encodeURIComponent(route)}/receipts/${encodeURIComponent(receipt)}/checks`
export function createKeywordRulesClient({requestResource}:Pick<Client,'requestResource'>){return {
 list:()=>requestResource<SensitiveWords>(words),
 save:(input:SensitiveWordsUpdate)=>requestResource<{revision:number}>(words,'PUT',input),
 checks:(route:string,receipt:string,page=1)=>requestResource<{items:KeywordCheck[];has_more:boolean}>(`${checks(route,receipt)}?page=${page}`),
 hits:(route:string,receipt:string,check:string,page=1)=>requestResource<KeywordHits>(`${checks(route,receipt)}/${encodeURIComponent(check)}/hits?page=${page}`),
}}

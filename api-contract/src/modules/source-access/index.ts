import {requestResource} from '../../http'
export interface SourceRow {id:string;unit_name?:string;business_unit_id?:string;business_unit_name?:string;route_name?:string;remark?:string;status:string;revision:number;key_id?:string;execution_state?:'draft'|'running'|'paused';retired_at?:string|null;config_revision?:number;valid_from?:string;valid_until?:string|null;last_used_at?:string|null;created_at:string;updated_at:string}
export interface Association {route_id:string;route_name:string;remark:string;status:string;revision:number}
export interface SourceInput {key_id?:string;unit_name?:string;business_unit_name?:string;remark?:string;status?:string;revision?:number;valid_from?:string;valid_until?:string|null;route_ids?:string[]}
export interface CredentialSecret {id:string;key_id:string;secret:string}
export type SourceKind='source-credentials'|'source-routes'
export interface SourceRouteMutation {id:string;business_unit_id:string}
function save(kind:'source-routes',v:SourceInput,id?:string):Promise<SourceRouteMutation>
function save(kind:'source-credentials',v:SourceInput,id?:string):Promise<{id:string}>
function save(kind:SourceKind,v:SourceInput,id?:string):Promise<{id:string;business_unit_id?:string}> {
 return requestResource(`/api/v2/admin/${kind}${id?'/'+encodeURIComponent(id):''}`,id?'PATCH':'POST',v)
}
export const sourceAccess={
 revealSecret:(id:string)=>requestResource<CredentialSecret>(`/api/v2/admin/source-credentials/${encodeURIComponent(id)}/secret-reveals`,'POST',{}),
 associate:(key:string,route:string,revision:number,status:'active'|'revoked')=>requestResource(`/api/v2/admin/source-credentials/${encodeURIComponent(key)}/routes/${encodeURIComponent(route)}`,'PUT',{revision,status}),
 list:(kind:SourceKind,page=1,search='',status='',key_id='',valid_until='')=>requestResource<{items:SourceRow[];has_more:boolean}>(`/api/v2/admin/${kind}?${new URLSearchParams({page:String(page),search,status,...(key_id?{key_id}:{}),...(valid_until?{valid_until}:{})})}`),
 save,
 remove:(id:string)=>requestResource(`/api/v2/admin/source-credentials/${encodeURIComponent(id)}`,'DELETE'),
 routes:(id:string,page=1)=>requestResource<{items:Association[];has_more:boolean}>(`/api/v2/admin/source-credentials/${encodeURIComponent(id)}/routes?page=${page}`),
}

export type {SourceEventV2,SourceReceiptV2,SourceCheckV2,SourceCapabilitiesV2,SourceDocument} from './protocol'
export * from './routing'

export * from './receipts'

export * from './keywords'

export * from './automation'

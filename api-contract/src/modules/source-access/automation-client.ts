import type {Client} from '../../http/client'
export interface MessageDecision {selected_ad:{material_id:string;placement_id:string;content:{name:string;body:string;link:string|null}}|null;reason:string;selection_count:number;content:string}
export interface BusinessMessage {id:string;platform_id:string;business_unit_id:string;receipt_id:string;event_id:string;created_at:string;state:string;reason_code:string|null;content:string|null;decision:MessageDecision|null;sending:{pending:number;sent:number;failed:number;unknown:number}|null;delegates:{id:string;channel:'mobile'|'wecom';task_id:string|null;configuration_id:string;target_count:number;state:string;reason_code:string|null}[]}
export function createMessageClient({requestResource}:Pick<Client,'requestResource'>){return {
 list:(route='',receipt='',page=1,size=30)=>requestResource<{items:BusinessMessage[];has_more:boolean}>(`/api/v2/admin/messages?${[...(route?[`route_id=${encodeURIComponent(route)}`]:[]),...(receipt?[`receipt_id=${encodeURIComponent(receipt)}`]:[]),`page%5Bnumber%5D=${page}`,`page%5Bsize%5D=${size}`].join('&')}`),
}}

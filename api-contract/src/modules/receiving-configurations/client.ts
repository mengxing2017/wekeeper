import type {Client} from '../../http/client'
export type ReceivingChannel = 'mobile' | 'wecom'
export interface ReceivingConfigurationInput {
  name:string; enabled:boolean; target_ids:string[]; device_ids:string[]; sender_ids:string[]
}
export interface ReceivingConfiguration extends ReceivingConfigurationInput {id:string; business_unit_id:string}
export interface ReceivingTargetInput {name:string;directory_group_id:string|null}
export interface ReceivingResources {directory_groups:{id:string;name:string}[];targets:{id:string;name:string}[];devices:{id:string;name:string}[];senders:{id:string;name:string}[]}
/** Enabling a unit configuration explicitly authorizes automatic dispatch of new messages. */
export function createReceivingConfigurationClient({requestResource,requestCollection}:Pick<Client,'requestResource'|'requestCollection'>,channel:ReceivingChannel) {
 const base=(unit:string)=>`/api/v1/${channel==='mobile'?'mobile-execution':'wecom-distribution'}/business-units/${encodeURIComponent(unit)}`
 return {
 list:(unit:string,page=1)=>requestCollection<ReceivingConfiguration>(`${base(unit)}/receiving-configurations?page[number]=${page}&page[size]=30`),
 get:(unit:string,id:string)=>requestResource<ReceivingConfiguration>(`${base(unit)}/receiving-configurations/${encodeURIComponent(id)}`),
 save:(unit:string,id:string,input:ReceivingConfigurationInput)=>requestResource<ReceivingConfiguration>(`${base(unit)}/receiving-configurations/${encodeURIComponent(id)}`,'PUT',input),
 target:(unit:string,id:string,input:ReceivingTargetInput)=>requestResource<{id:string;name:string}>(`${base(unit)}/receiving-targets/${encodeURIComponent(id)}`,'PUT',input),
 resources:(unit:string)=>requestResource<ReceivingResources>(`${base(unit)}/receiving-resources`),
 }
}

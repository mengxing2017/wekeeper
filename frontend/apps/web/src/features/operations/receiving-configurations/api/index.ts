import {mobileReceivingConfigurations,wecomReceivingConfigurations,type ReceivingChannel} from '@wekeeper/api-contract/receiving-configurations'
import {businessDirectory} from '@wekeeper/api-contract/business-directory'
export type {ReceivingChannel,ReceivingConfiguration,ReceivingConfigurationInput} from '@wekeeper/api-contract/receiving-configurations'
export const receivingConfigurations=(channel:ReceivingChannel)=>channel==='mobile'?mobileReceivingConfigurations:wecomReceivingConfigurations
export async function receivingUnits(){
 const units:{id:string;name:string}[]=[]
 for(let page=1;;page++) {const platforms=await businessDirectory.platforms(page)
  for(const platform of platforms.items) for(let unitPage=1;;unitPage++) {const result=await businessDirectory.units(platform.id,unitPage);units.push(...result.items.map(u=>({id:u.id,name:`${platform.name} / ${u.name}`})));if(!result.has_more)break}
  if(!platforms.has_more)break
 }
 return units
}

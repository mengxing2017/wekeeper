import type { SharedPreview, SharedOverview, SharedReadiness } from './api'

export function previewSelections(items: SharedPreview['plan']['items'], target: string, device: string) {
  return [...items.filter(i=>i.selected && i.target_id!==target).map(i=>({target_id:i.target_id,device_id:i.selected!.device_id})),{target_id:target,device_id:device}]
}

export function previewResourcesChanged(preview:SharedPreview, overview:SharedOverview|undefined) {
  return preview.plan.items.some(item=>(item.readiness??[]).some(snapshot=>{
    const current=overview?.readiness.find(r=>r.binding_id===snapshot.binding_id)
    return current && (current.search_text!==snapshot.search_text || current.plan_selectable!==snapshot.plan_selectable)
  }))
}
export function isUsageBlocker(code:string) {
  return ['business_scope_paused','business_scope_revoked','business_scope_expired'].includes(code)
}

export function currentReadiness(snapshot:SharedReadiness,overview:SharedOverview|undefined,synchronized:boolean) {
  return synchronized ? overview?.readiness.find(r=>r.binding_id===snapshot.binding_id) : undefined
}

export function dispatchLabel(state:string) {
  return ({scheduling:'平台已受理，正在调度',assigned:'已分配可用候选手机',waiting_resources:'已受理，持续寻找可用候选手机',executing:'手机正在处理本次尝试',completed:'目标结果已记录',needs_review:'本目标结果待核对，其他目标继续调度',stopped:'已停止调度',confirmation_expired:'业务确认已到期'} as Record<string,string>)[state]??'正在同步调度状态'
}

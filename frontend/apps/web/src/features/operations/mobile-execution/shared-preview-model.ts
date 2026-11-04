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

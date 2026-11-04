import type { SharedPreview, SharedOverview, SharedReadiness } from './api'

export function previewSelections(items: SharedPreview['plan']['items'], target: string, device: string) {
  return [...items.filter(i=>i.selected && i.target_id!==target).map(i=>({target_id:i.target_id,device_id:i.selected!.device_id})),{target_id:target,device_id:device}]
}

export function previewConfigurationChanged(preview:SharedPreview, overview:SharedOverview|undefined) {
  const snapshot=preview.plan.configuration
  const current=overview?.configuration
  if(snapshot&&current&&(snapshot.available!==current.available||snapshot.configuration_id!==current.configuration_id||snapshot.flow_digest!==current.flow_digest))return true
  return preview.plan.items.some(item=>(item.readiness??[]).some(snapshot=>{
    const current=overview?.readiness.find(r=>r.binding_id===snapshot.binding_id)
    return current && (current.configuration_id!==snapshot.configuration_id || current.configuration_digest!==snapshot.configuration_digest)
  }))
}
export function isUsageBlocker(code:string) {
  return ['business_scope_paused','business_scope_revoked','business_scope_expired'].includes(code)
}

export function currentReadiness(snapshot:SharedReadiness,overview:SharedOverview|undefined,synchronized:boolean) {
  return synchronized ? overview?.readiness.find(r=>r.binding_id===snapshot.binding_id) : undefined
}
export function configurationIssueMessage(code:string|undefined) {
  const messages:Record<string,string>={
    configuration_missing:'平台运行配置未安装，需系统修复',
    configuration_structure_invalid:'平台运行配置结构错误，需系统修复',
    configuration_engine_unsupported:'平台运行配置与执行引擎不匹配，需系统修复',
    configuration_constraints_invalid:'平台运行配置约束不合法，需系统修复',
    configuration_digest_mismatch:'平台运行配置摘要不一致，需系统修复',
  }
  return code ? messages[code]??'平台运行配置不可用，需系统修复' : '平台配置诊断待同步'
}

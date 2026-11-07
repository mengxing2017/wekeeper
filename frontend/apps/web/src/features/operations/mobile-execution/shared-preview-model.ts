import type { SharedPreview, SharedOverview, SharedReadiness, SharedTask, SharedAttempt } from './api'

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
  return ({scheduling:'平台已受理，正在调度',assigned:'已分配可用候选手机',waiting_resources:'本轮暂无可用候选，将在轮次上限内重试',dispatch_failed:'任务下发失败：候选手机已遍历 3 轮，已停止自动下发',executing:'手机正在处理本次尝试',completed:'目标结果已记录',needs_review:'本目标结果待核对，其他目标继续调度',stopped:'已停止调度',confirmation_expired:'业务确认已到期'} as Record<string,string>)[state]??'正在同步调度状态'
}

export function taskNeedsPolling(task:SharedTask|undefined):boolean {
  if(task?.status && !task.items?.length)return ['dispatching','needs_review'].includes(task.status)
  if(task?.items?.length && task.items.every(item=>item.dispatch?.state==='completed'))return false
  if(task?.stopped)return false
  if(!task?.items?.length)return true
  return task.items.some(item=>item.dispatch
    ? !['completed','stopped','confirmation_expired','dispatch_failed'].includes(item.dispatch.state)
    : !task.stopped && item.attempts.some(a=>a.current && ['queued','leased','prepared','invocation_issued'].includes(a.state)))
}

// Automatic dispatch termination is separate from the manual task stop flag.
export function taskProgressLabel(task:SharedTask):string|undefined {
  if(task.status)return ({dispatching:'任务正在调度',completed:'任务已完成',stopped:task.stopped?'任务已手动停止':'任务已停止自动下发',dispatch_failed:'任务已停止自动下发：三轮下发失败',confirmation_expired:'任务确认已到期，已停止自动下发',needs_review:'任务结果待核对'})[task.status]
  if(task.items?.length && task.items.every(item=>item.dispatch?.state==='completed'))return '任务已完成'
  if(task.stopped)return '任务已手动停止'
  if(!task.items?.length)return undefined
  if(taskNeedsPolling(task))return '任务正在调度'
  if(task.items.some(item=>!item.dispatch))return undefined
  if(task.items.every(item=>item.dispatch?.state==='completed'))return '任务已完成'
  if(task.items.some(item=>item.dispatch?.state==='dispatch_failed'))return '任务已停止自动下发：三轮下发失败'
  return '任务已停止自动下发'
}

export function attemptDispatchLabel(attempt:SharedAttempt,state:string|undefined,stopped:boolean):string|undefined {
  if(!attempt.current || ['observed_sent','manually_confirmed_sent'].includes(attempt.state))return undefined
  if(state==='dispatch_failed')return '已停止自动下发（已耗尽 3 轮）'
  if(stopped || state==='stopped')return '已停止自动下发'
  if(state==='confirmation_expired')return '业务确认已到期，已停止自动下发'
  if(state==='completed')return '目标已完成，本次尝试不再下发'
  return undefined
}

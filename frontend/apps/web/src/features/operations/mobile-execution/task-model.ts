import type { ExecutionTaskDetail, TaskItemState } from './api'

export function messageProblem(message: string): string | undefined {
  if (!message.trim()) return '请填写消息正文'
  if (new TextEncoder().encode(message).length > 4000) return '消息超过 4000 UTF-8 字节，请缩短正文'
  if ([...message].some(c => /\p{Cc}/u.test(c) && !['\n', '\r', '\t'].includes(c))) return '消息含有不支持的控制字符'
}
export const taskItemLabels: Record<TaskItemState, string> = {
  pending: '尚未领取', claimed: '已领取，尚未准备发送', prepared: '已准备，尚未发放调用',
  invocation_issued: '调用已发，等待回报', reported_sent: '设备报告已发（不代表送达）',
  verified_not_sent: '确定未发送', unknown: '结果不确定，待核对', failed_before_send: '发送前失败',
  invalidated: '配置失效，未执行', cancelled: '已取消，未执行',
}
export const taskLabels: Record<ExecutionTaskDetail['state'], string> = {
  queued: '待执行', running: '执行中', paused: '已暂停', completed: '已结束', partially_completed: '部分完成', cancelled: '已取消',
}
export function taskCounts(task: ExecutionTaskDetail) {
  const count = (...states: TaskItemState[]) => task.items.filter(i => states.includes(i.state)).length
  return `报告已发 ${count('reported_sent')}；确定未发/失败 ${count('verified_not_sent', 'failed_before_send')}；未执行 ${count('pending', 'claimed', 'prepared', 'invalidated', 'cancelled')}；等待回报 ${count('invocation_issued')}；待核对 ${count('unknown')}`
}
export function canCancelTask(task: ExecutionTaskDetail) {
  return task.state === 'queued' && task.items.length > 0 && task.items.every(i => i.state === 'pending')
}
export function taskError(error: unknown): string {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : ''
  const messages: Record<string, string> = {
    mobile_device_reserved: '这台手机已有任务占用。请先处理原任务；其他学校的占用请联系平台管理员协调，不能另选学校绕过。',
    mobile_target_unresolved: '所选目标已有待执行或待核对项。请在任务记录中处理原任务；结果不确定时先核对，不要换设备重建任务。',
    mobile_execution_configuration_required: '该手机尚未准备好。请在“手机执行配置”检查学校授权、手机接受状态、流程发布和现场验证。',
    mobile_target_binding_required: '目标已停用、解绑或不属于所选手机。请刷新目标，并联系学校管理员在“手机目标群”检查绑定。',
    mobile_preview_expired: '预览已过期。请关闭预览，重新预览全部目标后再确认。',
    revision_conflict: '授权、定位或执行配置已变化。请关闭预览，刷新配置和目标后重新预览，不能沿用旧确认。',
    mobile_task_not_cancellable: '任务版本或执行状态已变化，不能按未领取任务取消。请刷新任务记录；已发调用和不确定结果需要核对。',
    forbidden: '当前学校权限已变化。请重新选择有权限的学校；人工确认需要本校管理员权限。',
    conflict: '预览可能已确认或任务存在冲突。请刷新任务记录核实原操作结果，不要重复创建。',
    unauthorized: '登录已失效，请重新登录后查看任务记录，核实原操作结果。',
  }
  return messages[code] || (error instanceof Error ? error.message : '操作失败，请刷新后重试')
}

export function canConfirmPreview(expiresAt: string, now: number, attempted: boolean) {
  return attempted || Date.parse(expiresAt) > now
}

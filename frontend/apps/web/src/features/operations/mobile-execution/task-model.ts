export function taskError(error: unknown): string {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : ''
  const messages: Record<string, string> = {
    shared_send_execution_configuration_invalid: '当前发送配置异常，本次未执行。可报告问题，修复后重新预览。',
    forbidden: '当前学校权限已变化。请重新选择有权限的学校；人工确认需要本校管理员权限。',
    unauthorized: '登录已失效，请重新登录后查看任务记录，核实原操作结果。',
  }
  return messages[code] || (error instanceof Error ? error.message : '操作失败，请刷新后重试')
}

export function attemptFailureReason(reason:string):string {
  const messages:Record<string,string>={
    page_unrecognized:'手机无法识别当前企业微信页面',
    page_transition_timeout:'企业微信页面未在时限内完成切换',
    ambiguous_page:'当前页面存在多种可能解释',
    ambiguous_control:'当前操作控件无法唯一识别',
    shared_send_adapter_unavailable:'手机本地执行能力暂不可用',
    local_adapter_invalid:'手机本地执行资源异常',
    control_unavailable:'当前页面缺少可操作控件',
    send_unavailable:'当前消息发送控件不可用',
    send_observation_unknown:'手机无法确认本次发送结果，请核对，勿重复发送',
  }
  return messages[reason]??'手机未完成本次执行，请查看手机执行记录'
}

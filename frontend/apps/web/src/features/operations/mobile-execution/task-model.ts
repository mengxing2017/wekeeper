import type { SharedCandidate } from './api'

export function candidateGroupName(candidate: SharedCandidate): string {
  return candidate.target.search_query
}

export function taskError(error: unknown): string {
  const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : ''
  const messages: Record<string, string> = {
    dispatch_round_limit_exceeded:'任务已停止调度，请复制原意图为新草稿并重新确认。',
    dispatch_candidate_already_visited:'该手机本轮已尝试，请等待平台按剩余轮次自动调度。',
    shared_send_execution_configuration_invalid: '当前发送配置异常，本次未执行。可报告问题，修复后重新预览。',
    forbidden: '当前账号没有全局系统管理员权限，请核对账号状态与权限。',
    unauthorized: '登录已失效，请重新登录后查看任务记录，核实原操作结果。',
  }
  return messages[code] || (error instanceof Error ? error.message : '操作失败，请刷新后重试')
}

export function attemptFailureReason(reason:string | null | undefined):string {
  const messages:Record<string,string>={
    adapter_unavailable:'手机本地适配资源缺失',
    adapter_budget:'手机本地适配资源超出预算',
    unsupported_adapter_schema:'手机不支持当前适配资源格式',
    invalid_adapter_path:'手机适配资源路径无效',
    invalid_adapter_json:'手机适配资源格式无效',
    unsupported_protocol:'手机本地执行协议不受支持',
    invalid_message:'执行包正文不符合消息约束',
    execution_identity_mismatch:'执行身份不匹配，请核对双端执行包',
    execution_already_started:'本次执行已经开始或结束，不会重复执行',
    capability_already_consumed:'本次发送资格已消费，不会重复调用',
    invalid_permit_window:'本次执行授权时间范围无效',
    shared_target_changed:'准备过程中目标群发生变化',
    shared_preparation_changed:'执行前的页面或正文准备发生变化',
    chat_title_required:'无法读取当前会话标题',
    chat_title_mismatch:'当前会话标题与目标不匹配',
    chat_changed:'执行过程中会话页面发生变化',
    unexpected_page:'当前页面与预期执行页面不匹配',
    input_failed:'手机未能完成正文输入',
    existing_draft:'当前页面存在未处理草稿',
    existing_search:'当前页面存在未处理的搜索内容',
    search_input_unconfirmed:'未能确认目标群搜索内容',
    search_results_unstable:'目标群搜索结果尚未稳定',
    group_results_changed:'群搜索结果在操作前发生变化',
    group_target_not_found:'未找到目标群',
    navigation_budget:'目标导航预算已耗尽',
    navigation_no_progress:'目标导航未取得进展',
    send_action_unconfirmed:'无法确认发送动作结果，请核对原尝试',
    node_read_failed:'未能读取当前页面节点',
    node_query_timeout:'页面节点查询超时',
    notifications_required:'本次运行所需通知权限不可用',
    secure_locked:'手机当前处于安全锁定状态',
    waiting_user_unlock:'手机当前等待本人解锁',
    application_not_found:'手机未找到目标应用',
    application_launch_denied:'系统未允许打开目标应用',
    launch_not_confirmed:'未能确认目标应用已打开',
    recovery_budget_exceeded:'本地页面恢复预算已耗尽',
    recovery_no_progress:'本地页面恢复未取得进展',
    recovery_cycle_detected:'本地页面恢复出现循环',
    recovery_observation_unavailable:'本地页面恢复缺少可用观测',
    recovery_unowned_edit:'本地恢复发现非本次执行的编辑内容',
    recovery_unsaved_changes:'本地恢复发现未保存内容',
    recovery_home_unconfirmed:'未能确认已恢复到应用主页',
    device_unavailable:'手机本次暂不可执行，请在手机本地查看原执行记录',
    process_restarted:'执行完成前进程重启，本次按原调用事实结算',
    shared_runtime_not_ready:'手机当前运行环境不满足本次执行',
    accessibility_required:'本次执行未能使用无障碍服务',
    wecom_business_page_required:'未能准备企业微信业务页面',
    wechat_not_logged_in:'个人微信当前未登录',
    wechat_verification_required:'个人微信当前需要本人验证',
    wecom_not_logged_in:'企业微信当前未登录',
    wecom_verification_required:'企业微信当前需要本人验证',
    adapter_incompatible:'手机本地适配与当前应用版本不匹配',
    adapter_digest_mismatch:'手机本地适配资源摘要不匹配',
    application_instance_ambiguous:'无法唯一确定本次执行的应用实例',
    unsupported_application:'当前应用不受本地执行适配支持',
    unsupported_application_version:'当前应用版本不受本地执行适配支持',
    target_routes_exhausted:'目标群定位路径已耗尽',
    group_target_not_found_all_routes:'所有定位路径均未找到目标群',
    target_uniqueness_unproven:'无法证明目标群唯一，本次未发送',
    plan_digest_mismatch:'执行计划摘要不匹配，请核对双端执行包',
    invalid_device_execution_contract:'设备执行契约无效，请核对双端版本与执行包',
    unsupported_device_execution:'设备不支持本次执行契约',
    permit_expired:'本次单次执行授权已过期',
    send_time_limit:'手机本地执行预算已耗尽',
    local_execution_failed:'手机本地执行异常，请查看同一执行标识的手机记录',
    execution_cancelled:'本次执行已取消',
    new_outgoing_observed:'手机已观察到新发出的消息',
    execution_not_started:'本次执行尚未开始即结束',

    dispatch_round_limit_exceeded:'候选手机已遍历 3 轮，已停止自动下发',
    delivery_not_received_timeout:'旧尝试未在其接收期限内收到回执',
    receipt_timeout:'60 秒内未收到执行包落盘确认，平台按授权候选继续调度',
    result_timeout:'执行包接收确认后 5 分钟未收到最终结果',
    dispatch_candidate_already_visited:'该手机本轮已尝试，平台将按剩余预算自动调度',
    plan_dependency_changed:'该尝试的有效目标或授权已变化，其他合法任务继续调度',
    target_identity_required:'设备未能确定目标群身份，本次尝试未发送',
    retired_target_contract:'该尝试采用已停用的目标格式，未发放执行许可',
    execution_contract_replaced:'该尝试采用已停用的执行契约，未发放执行许可',
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
  const code = (reason ?? '').replace(/[\p{Cc}\p{Cf}]/gu, '').trim().slice(0, 200)
  if (!code) return '未提供具体原因'
  const label = Object.prototype.hasOwnProperty.call(messages, code) ? messages[code] : '手机未完成本次执行，请查看手机执行记录'
  return `${label}（原因码：${code}）`
}

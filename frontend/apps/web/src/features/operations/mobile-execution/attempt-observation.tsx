import type { SharedAttempt } from './api'

// The zone is explicit; device wall clocks are observations, never dispatch clocks.
export function observationTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return '时间格式无效'
  return `${date.toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', hour12: false })}（UTC+08:00）`
}

export function AttemptObservation({ attempt: a }: { attempt: SharedAttempt }) {
  return <small>
    ；尝试标识：{a.id}
    {a.execution_id && <>；执行标识：{a.execution_id}</>}
    {a.inbox_received_at && <>；任务列表同步回执（平台记录）：{observationTime(a.inbox_received_at)}</>}
    {a.execution_accepted_at && <>；执行包落盘确认（平台记录）：{observationTime(a.execution_accepted_at)}</>}
    {a.result_received_at && <>；执行结果收报（平台记录）：{observationTime(a.result_received_at)}{!a.execution_accepted_at && '；结果已收到，未单独记录落盘确认'}</>}
    {a.execution_id && <>；设备本地开始时间：{a.device_started_at ? `${observationTime(a.device_started_at)}，未校时` : '未提供'}；设备本地结束时间：{a.device_finished_at ? `${observationTime(a.device_finished_at)}，未校时` : '未提供'}</>}
  </small>
}

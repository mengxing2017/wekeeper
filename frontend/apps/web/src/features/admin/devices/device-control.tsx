import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Space } from 'antd'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { api, type DeviceControlKind } from './api'
import { ControlStatus, useControlFreshness } from './control-status'
const commands = { pause_execution: '暂停执行', resume_execution: '解除平台暂停', request_status: '查询状态', task_available: '任务通知', configuration_changed: '配置通知' }
const receipts: Record<string, string> = { queued: '等待投递', delivered: '等待接收确认', accepted: '设备已落盘', completed: '控制动作已完成', rejected: '设备已拒绝', expired: '已过期' }
const reasons: Record<string, string> = { platform_pause: '平台已暂停执行', execution_authorization_required: '等待业务执行授权', automatic_execution_paused: '手机端已暂停执行', device_locked: '等待手机解锁', accessibility_required: '等待开启无障碍服务', notification_required: '等待开启通知权限', unknown_requires_review: '存在未知结果，等待人工核对' }
export function DeviceControlPanel({ deviceId }: { deviceId: string }) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const query = useQuery({ queryKey: ['mobile', 'platform', 'control', deviceId], queryFn: () => api.controlPresence(deviceId), refetchInterval: 5000, refetchOnWindowFocus: 'always' })
  const value = query.data
  const fresh = useControlFreshness(query.dataUpdatedAt, query.isError)
  async function send(kind: DeviceControlKind) {
    if (busy) return
    setBusy(true); setError('')
    try { await api.controlCommand(deviceId, kind, createIdempotencyKey()); await query.refetch() }
    catch (e) { setError((e as Error).message) }
    finally { setBusy(false) }
  }
  return <section>
    <h3>长期托管连接</h3>
    {query.error && <Alert type="error" title={query.error.message} />}
    {value && <><ControlStatus state={value.state} fresh={fresh} />
      {!fresh && <p>状态获取失败或已过期，上次确认于：{new Date(query.dataUpdatedAt).toLocaleString()}</p>}
      <p>控制通道：{value.transport ?? '尚未建立'} · 最近心跳：{value.last_control_heartbeat_at ?? '尚未连接'}</p>
      <p>执行条件：{value.readiness === 'ready' ? '就绪' : (value.block_reason ? reasons[value.block_reason] ?? '设备尚未就绪' : '等待设备上报')} · 平台期望：{value.desired_paused ? '暂停执行' : '允许按原授权评估执行'}</p>
      <p>凭据到期：{value.credential_expires_at ?? '无有效凭据'}。在线不代表已获准发送；离线指令将等待设备恢复后确认。</p>
      <Space wrap><Button disabled={busy} onClick={() => void send('pause_execution')}>暂停执行</Button><Button disabled={busy} onClick={() => void send('resume_execution')}>解除平台暂停</Button><Button disabled={busy} onClick={() => void send('request_status')}>请求状态</Button></Space>
      <p>解除平台暂停不会解除手机紧急停止、业务授权暂停或未知结果核对。</p>
      <ul>{value.commands.slice(0, 10).map((c) => <li key={c.id}>{commands[c.kind]} · {receipts[c.state] ?? c.state}{c.result_code ? ` · ${c.result_code}` : ''}</li>)}</ul>
    </>}
    {error && <Alert type="error" title={error} />}
  </section>
}

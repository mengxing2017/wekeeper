import { useEffect, useState } from 'react'
import { Tag } from 'antd'
import type { DeviceControlPresence } from './api'

const labels = { connecting: '正在连接', online: '在线', suspect: '连接不稳定', offline: '离线', disabled: '设备已停用', unauthorized: '设备凭据已失效' }

// Polling may stop in a hidden tab. Cached success must not remain a live claim.
export function useControlFreshness(updatedAt: number, failed: boolean) {
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    const tick = () => setNow(Date.now())
    const timer = window.setInterval(tick, 1000)
    window.addEventListener('visibilitychange', tick)
    return () => { window.clearInterval(timer); window.removeEventListener('visibilitychange', tick) }
  }, [])
  return !failed && updatedAt > 0 && now - updatedAt < 15_000
}

export function ControlStatus({ state, fresh }: { state?: DeviceControlPresence['state']; fresh: boolean }) {
  return <Tag color={fresh && state === 'online' ? 'success' : 'warning'}>
    {!fresh ? '状态待刷新' : state ? labels[state] : '等待设备上报'}
  </Tag>
}

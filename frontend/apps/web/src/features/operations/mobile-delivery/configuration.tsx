import { useState } from 'react'
import { Alert, Button } from 'antd'
import { DeviceCapabilities } from '../../admin/mobile-resources/capabilities'
import { LegacyMaintenanceHistory } from './dispatch/workbench'

export function ConfigurationWorkbench() {
  const [history, setHistory] = useState(false)
  return <>
    <Alert type="info" title="共享手机业务能力开通" description="原高级适配入口已退出。无需选择试用学校或发送账号；开通、手机同意和现场检查不代表已允许发送，共享任务执行链路尚未完成。" />
    <DeviceCapabilities />
    <Button onClick={() => setHistory(!history)}>{history ? '关闭历史 v5 资料' : '查看历史 v5 资料（只读）'}</Button>
    {history && <LegacyMaintenanceHistory />}
  </>
}

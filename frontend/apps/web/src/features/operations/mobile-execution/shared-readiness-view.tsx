import type { SharedReadiness } from './api'

export function SharedReadinessView({snapshot,current,blocked}:{snapshot:SharedReadiness;current:SharedReadiness|undefined;blocked:boolean}) {
  const phone=current??snapshot
  const fact=(value:string)=>value==='yes'?'是':value==='no'?'否':'未知'
  return <>
    <p>已绑定手机：{phone.device_name}</p>
    {blocked&&<details><summary>受理诊断</summary>
      {snapshot.blockers.map(b=><p key={b.code}>诊断代码：{b.code}</p>)}
    </details>}
    <p>{current ? '当前平台观察（独立于受理预览）' : '以下为预览快照'}</p>
    <p>连接：{phone.online===true?'在线':phone.online===false?'离线':'尚无观察'}</p>
    {phone.environment&&<details><summary>环境观测</summary>
      <p>观测时间：{new Date(phone.environment.observed_at).toLocaleString()}，仅反映该次观测。</p>
      <p>屏幕可交互：{fact(phone.environment.interactive)}；设备锁定：{fact(phone.environment.device_locked)}</p>
      <p>无障碍可用：{fact(phone.environment.accessibility)}；通知可用：{fact(phone.environment.notification)}</p>
    </details>}
    {(phone.execution_conditions??[]).map(c=><p key={c.code}>执行状态：{c.message}</p>)}
  </>
}

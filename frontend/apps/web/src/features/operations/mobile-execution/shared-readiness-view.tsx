import type { SharedReadiness } from './api'

export function SharedReadinessView({snapshot,current,blocked}:{snapshot:SharedReadiness;current:SharedReadiness|undefined;blocked:boolean}) {
  const phone=current??snapshot
  return <>
    <p>已绑定手机：{phone.device_name}</p>
    {blocked&&<details><summary>受理诊断</summary>
      {snapshot.blockers.map(b=><p key={b.code}>诊断代码：{b.code}</p>)}
    </details>}
    <p>{current ? '实时手机状态（独立于受理预览）' : '当前手机状态待同步，以下为预览快照'}</p>
    <p>手机托管：{phone.consent_state==='accepted'?'已同意':phone.consent_state==='revoked'?'已撤回':'状态待同步，不代表未授权'}</p>
    {phone.runtime_observed_at&&<p>{current?'手机状态上报时间':'预览中的手机上报时间'}：{new Date(phone.runtime_observed_at).toLocaleString()}</p>}
    {phone.preparation?.observed_at&&<p>手机准备状态时间：{new Date(phone.preparation.observed_at).toLocaleString()}</p>}
    {(phone.execution_conditions??[]).map(c=><p key={c.code}>执行状态：{c.message}</p>)}
  </>
}

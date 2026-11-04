import type { SharedReadiness, SharedConfigurationStatus } from './api'
import { configurationIssueMessage } from './shared-preview-model'

export function SharedReadinessView({snapshot,current,configuration,blocked}:{snapshot:SharedReadiness;current:SharedReadiness|undefined;configuration:SharedConfigurationStatus|undefined;blocked:boolean}) {
  const phone=current??snapshot
  return <>
    <p>已绑定手机：{phone.device_name}</p>
    {blocked&&<details><summary>受理诊断</summary>
      {snapshot.blockers.map(b=><p key={b.code}>诊断代码：{b.code}</p>)}
      {snapshot.configuration_issue&&<><p>配置根因：{configurationIssueMessage(snapshot.configuration_issue.code)}</p><p>配置问题：{snapshot.configuration_issue.code} / {snapshot.configuration_issue.path}</p></>}
      <p>预览配置标识：{snapshot.configuration_id??'未取得'}</p>
      <p>当前配置标识：{configuration ? configuration.configuration_id??'未安装' : current ? current.configuration_id??'未取得' : '待同步，不能用预览快照判断'}</p>
      {configuration&&<><p>平台配置版本：{configuration.version??'未安装'}</p><p>平台配置观测时间：{new Date(configuration.observed_at).toLocaleString()}</p><p>配置来源：平台运行配置（服务端）</p></>}
    </details>}
    <p>{current ? '实时手机状态（独立于受理预览）' : '当前手机状态待同步，以下为预览快照'}</p>
    <p>手机托管：{phone.consent_state==='accepted'?'已同意':phone.consent_state==='revoked'?'已撤回':'状态待同步，不代表未授权'}</p>
    {phone.runtime_observed_at&&<p>{current?'手机状态上报时间':'预览中的手机上报时间'}：{new Date(phone.runtime_observed_at).toLocaleString()}</p>}
    {(phone.execution_conditions??[]).map(c=><p key={c.code}>执行状态：{c.message}</p>)}
  </>
}

import {expect,it} from 'vitest'
import {renderToStaticMarkup} from 'react-dom/server'
import type {SharedReadiness,SharedConfigurationStatus} from './api'
import {SharedReadinessView} from './shared-readiness-view'
const snapshot={device_name:'测试手机',blockers:[],configuration_id:null,configuration_issue:{code:'configuration_missing',path:'flow'},consent_state:'accepted',runtime_observed_at:'2026-11-04T04:10:09Z'} as unknown as SharedReadiness
const state={capability:'wecom.group_text',source:'platform_runtime_configuration',observed_at:'2026-11-04T04:11:00Z',version:'real-version',flow_digest:'digest',configuration_id:'installed',available:true,issue:null} as SharedConfigurationStatus
it('实时查询失败时明确展示快照来源，不声称当前配置未取得',()=>{
 const html=renderToStaticMarkup(<SharedReadinessView snapshot={snapshot} current={undefined} configuration={undefined} blocked/>)
 expect(html).toContain('当前配置标识：待同步，不能用预览快照判断')
 expect(html).toContain('当前手机状态待同步，以下为预览快照')
 expect(html).toContain('预览中的手机上报时间')
 expect(html).not.toContain('实时手机状态（独立于受理预览）')
})
it('平台配置独立于手机绑定；预览缺失而当前已安装时分别展示',()=>{
 const html=renderToStaticMarkup(<SharedReadinessView snapshot={snapshot} current={undefined} configuration={state} blocked/>)
 expect(html).toContain('预览配置标识：未取得');expect(html).toContain('当前配置标识：installed')
 expect(html).toContain('平台配置观测时间');expect(html).toContain('平台运行配置（服务端）')
 expect(html).toContain('当前手机状态待同步，以下为预览快照')
})
it('当前配置已丢失时不沿用预览配置标识，保留手机同意事实',()=>{
 const html=renderToStaticMarkup(<SharedReadinessView snapshot={{...snapshot,configuration_id:'old'}} current={snapshot} configuration={{...state,configuration_id:null,available:false}} blocked/>)
 expect(html).toContain('预览配置标识：old');expect(html).toContain('当前配置标识：未安装')
 expect(html).toContain('手机托管：已同意');expect(html).toContain('配置根因：平台运行配置未安装，需系统修复')
})

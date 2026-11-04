import {expect,it} from 'vitest'
import {renderToStaticMarkup} from 'react-dom/server'
import type {SharedReadiness} from './api'
import {SharedReadinessView} from './shared-readiness-view'
const snapshot={device_name:'手机',blockers:[],consent_state:'accepted',runtime_observed_at:'2026-11-04T04:10:09Z'} as unknown as SharedReadiness
it('查询失败时展示快照来源，不要求平台安装手机配置',()=>{
 const html=renderToStaticMarkup(<SharedReadinessView snapshot={snapshot} current={undefined} blocked/>)
 expect(html).toContain('当前手机状态待同步，以下为预览快照')
 expect(html).toContain('预览中的手机上报时间')
 expect(html).not.toContain('平台运行配置')
 expect(html).not.toContain('配置标识')
})
it('保留手机托管事实',()=>{
 const html=renderToStaticMarkup(<SharedReadinessView snapshot={snapshot} current={snapshot} blocked/>)
 expect(html).toContain('手机托管：已同意')
 expect(html).not.toContain('配置根因')
})

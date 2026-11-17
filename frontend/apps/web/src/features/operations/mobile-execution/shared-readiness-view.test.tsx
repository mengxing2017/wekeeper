import {expect,it} from 'vitest'
import {renderToStaticMarkup} from 'react-dom/server'
import type {SharedReadiness} from './api'
import {SharedReadinessView} from './shared-readiness-view'
const snapshot={device_name:'手机',blockers:[],online:true} as unknown as SharedReadiness
it('查询失败时明确展示预览快照',()=>{
 const html=renderToStaticMarkup(<SharedReadinessView snapshot={snapshot} current={undefined} blocked/>)
 expect(html).toContain('以下为预览快照')
 expect(html).toContain('连接：在线')
 expect(html).not.toContain('托管')
})
it('只展示带观测时间的可选环境，不宣称可发送',()=>{
 const current={...snapshot,environment:{schema:'device_environment/1',observation_id:'observation',observed_at:'2026-11-18T04:10:09Z',interactive:'no',device_locked:'unknown',accessibility:'no',notification:'unknown'}} as SharedReadiness
 const html=renderToStaticMarkup(<SharedReadinessView snapshot={snapshot} current={current} blocked/>)
 expect(html).toContain('观测时间')
 expect(html).toContain('仅反映该次观测')
 expect(html).not.toContain('托管')
 expect(html).not.toContain('就绪')
})

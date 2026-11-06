import {expect,it} from 'vitest'
import {previewSelections,previewResourcesChanged,isUsageBlocker,currentReadiness} from './shared-preview-model'
import type {SharedPreview} from './api'
it('没有旧选中项的目标也会加入本次设备选择',()=>{
 const items=[{target_id:'target',selected:null},{target_id:'other',selected:{device_id:'other-phone'}}] as SharedPreview['plan']['items']
 expect(previewSelections(items,'target','new-phone')).toEqual([{target_id:'other',device_id:'other-phone'},{target_id:'target',device_id:'new-phone'}])
})
it('业务资源变化要求重新预览，手机状态和配置不影响平台确认',()=>{
 const snapshot={binding_id:'binding',search_text:'群名',target:{exact_name:'实际完整群名'},plan_selectable:true}
 const preview={plan:{items:[{readiness:[snapshot]}]}} as unknown as SharedPreview
 const overview=(row:unknown)=>({readiness:[row]}) as Parameters<typeof previewResourcesChanged>[1]
 expect(previewResourcesChanged(preview,overview({...snapshot,runtime_state:'screen_off',configuration_id:'new'}))).toBe(false)
 expect(previewResourcesChanged(preview,overview({...snapshot,search_text:'新群名'}))).toBe(true)
 expect(previewResourcesChanged(preview,overview({...snapshot,target:{exact_name:'另一完整群名'}}))).toBe(true)
 expect(previewResourcesChanged(preview,overview({...snapshot,plan_selectable:false}))).toBe(true)
 expect(isUsageBlocker('business_scope_paused')).toBe(true)
})
it('查询失败或绑定缺失时不把预览快照标为当前',()=>{
 const snapshot={binding_id:'binding'} as import('./api').SharedReadiness
 const overview={readiness:[snapshot]} as Parameters<typeof currentReadiness>[1]
 expect(currentReadiness(snapshot,overview,false)).toBeUndefined()
 expect(currentReadiness(snapshot,{readiness:[]} as unknown as Parameters<typeof currentReadiness>[1],true)).toBeUndefined()
 expect(currentReadiness(snapshot,overview,true)).toBe(snapshot)
})

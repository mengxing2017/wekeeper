import {expect,it} from 'vitest'
import {previewSelections,previewResourcesChanged,isUsageBlocker,currentReadiness,dispatchLabel,taskNeedsPolling} from './shared-preview-model'
import type {SharedPreview} from './api'
it('没有旧选中项的目标也会加入本次设备选择',()=>{
 const items=[{target_id:'target',selected:null},{target_id:'other',selected:{device_id:'other-phone'}}] as SharedPreview['plan']['items']
 expect(previewSelections(items,'target','new-phone')).toEqual([{target_id:'other',device_id:'other-phone'},{target_id:'target',device_id:'new-phone'}])
})
it('业务资源变化要求重新预览，手机状态和配置不影响平台确认',()=>{
 const snapshot={binding_id:'binding',search_text:'群名',plan_selectable:true}
 const preview={plan:{items:[{readiness:[snapshot]}]}} as unknown as SharedPreview
 const overview=(row:unknown)=>({readiness:[row]}) as Parameters<typeof previewResourcesChanged>[1]
 expect(previewResourcesChanged(preview,overview({...snapshot,runtime_state:'screen_off',configuration_id:'new'}))).toBe(false)
 expect(previewResourcesChanged(preview,overview({...snapshot,search_text:'新群名'}))).toBe(true)
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

it('三轮耗尽报告下发失败，资源等待说明轮次上限',()=>{
 expect(dispatchLabel('dispatch_failed')).toContain('已遍历 3 轮')
 expect(dispatchLabel('dispatch_failed')).toContain('已停止自动下发')
 expect(dispatchLabel('waiting_resources')).toContain('轮次上限')
})

it('所有目标终止后停止自动轮询，有其他活动目标时继续轮询',()=>{
 const task=(states:string[])=>({stopped:false,items:states.map(state=>({dispatch:{state}}))}) as unknown as Parameters<typeof taskNeedsPolling>[0]
 expect(taskNeedsPolling(task(['dispatch_failed','completed']))).toBe(false)
 expect(taskNeedsPolling(task(['dispatch_failed','assigned']))).toBe(true)
 expect(taskNeedsPolling(undefined)).toBe(true)
})

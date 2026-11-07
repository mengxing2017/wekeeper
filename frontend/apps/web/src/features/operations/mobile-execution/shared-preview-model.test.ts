import {expect,it} from 'vitest'
import {previewSelections,previewResourcesChanged,isUsageBlocker,currentReadiness,dispatchLabel,taskNeedsPolling,taskProgressLabel,attemptDispatchLabel} from './shared-preview-model'
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

it('自动耗尽独立于人工停止标记，部分目标失败不误报整任务停止',()=>{
 const task=(states:string[])=>({stopped:false,items:states.map(state=>({dispatch:{state}}))}) as unknown as Parameters<typeof taskProgressLabel>[0]
 expect(taskProgressLabel(task(['dispatch_failed','completed']))).toBe('任务已停止自动下发：三轮下发失败')
 expect(taskProgressLabel(task(['dispatch_failed','executing']))).toBe('任务正在调度')
 expect(taskProgressLabel(task(['completed']))).toBe('任务已完成')
 expect(taskNeedsPolling({...task(['executing']),stopped:true})).toBe(false)
})

it('终止目标不再把当前尝试的领取记录显示为仍在执行，历史结果保留',()=>{
 const attempt={current:true,state:'leased'} as Parameters<typeof attemptDispatchLabel>[0]
 expect(attemptDispatchLabel(attempt,'dispatch_failed',false)).toBe('已停止自动下发（已耗尽 3 轮）')
 expect(attemptDispatchLabel(attempt,'completed',false)).toBe('目标已完成，本次尝试不再下发')
 expect(attemptDispatchLabel({...attempt,current:false},'dispatch_failed',false)).toBeUndefined()
 expect(attemptDispatchLabel({...attempt,state:'observed_sent'},'dispatch_failed',false)).toBeUndefined()
})

it('采用接口汇总状态，完成优先于人工停止标记，不依赖详情缓存',()=>{
 const task=(status:NonNullable<Parameters<typeof taskProgressLabel>[0]['status']>,stopped=false)=>({status,stopped}) as Parameters<typeof taskProgressLabel>[0]
 expect(taskProgressLabel(task('completed',true))).toBe('任务已完成')
 expect(taskProgressLabel(task('stopped',true))).toBe('任务已手动停止')
 expect(taskProgressLabel(task('dispatch_failed'))).toBe('任务已停止自动下发：三轮下发失败')
 expect(taskNeedsPolling(task('completed'))).toBe(false)
 expect(taskNeedsPolling(task('dispatching'))).toBe(true)
})

it('汇总先记录完成但详情投影尚在刷新时，继续获取逐群最终状态',()=>{
 const task={status:'completed',stopped:false,items:[{dispatch:{state:'executing'}}]} as unknown as Parameters<typeof taskNeedsPolling>[0]
 expect(taskProgressLabel(task!)).toBe('任务已完成')
 expect(taskNeedsPolling(task)).toBe(true)
 expect(taskNeedsPolling({...task!,items:[{dispatch:{state:'completed'}}]} as Parameters<typeof taskNeedsPolling>[0])).toBe(false)
})

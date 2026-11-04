import {expect,it} from 'vitest'
import {previewSelections,previewConfigurationChanged,isUsageBlocker,currentReadiness,configurationIssueMessage} from './shared-preview-model'
import type {SharedPreview} from './api'
it('没有旧选中项的目标也会加入本次设备选择',()=>{
 const items=[{target_id:'target',selected:null},{target_id:'other',selected:{device_id:'other-phone'}}] as SharedPreview['plan']['items']
 expect(previewSelections(items,'target','new-phone')).toEqual([{target_id:'other',device_id:'other-phone'},{target_id:'target',device_id:'new-phone'}])
 expect(previewSelections(items,'other','replacement')).toEqual([{target_id:'other',device_id:'replacement'}])
})

it('配置身份变化要求重预览，手机运行状态变化不改写配置',()=>{
 const snapshot={binding_id:'binding',configuration_id:'one',configuration_digest:'digest'}
 const preview={plan:{items:[{readiness:[snapshot]}]}} as unknown as SharedPreview
 const overview=(row:unknown)=>({readiness:[row]}) as Parameters<typeof previewConfigurationChanged>[1]
 expect(previewConfigurationChanged(preview,overview({...snapshot,runtime_state:'screen_off'}))).toBe(false)
 expect(previewConfigurationChanged(preview,overview({...snapshot,configuration_id:'two'}))).toBe(true)
 expect(previewConfigurationChanged(preview,overview({...snapshot,configuration_digest:'new'}))).toBe(true)
 expect(isUsageBlocker('shared_send_execution_configuration_invalid')).toBe(false)
 expect(isUsageBlocker('business_scope_paused')).toBe(true)
})

it('失败或绑定缺失时不把预览快照标为当前，并允许未知诊断保守展示',()=>{
 const snapshot={binding_id:'binding',configuration_id:null} as import('./api').SharedReadiness
 const overview={readiness:[snapshot]} as Parameters<typeof currentReadiness>[1]
 expect(currentReadiness(snapshot,overview,false)).toBeUndefined()
 expect(currentReadiness(snapshot,{readiness:[]} as unknown as Parameters<typeof currentReadiness>[1],true)).toBeUndefined()
 expect(currentReadiness(snapshot,overview,true)).toBe(snapshot)
 expect(configurationIssueMessage('configuration_missing')).toBe('平台运行配置未安装，需系统修复')
 expect(configurationIssueMessage('future_issue')).toBe('平台运行配置不可用，需系统修复')
})
it('全局配置变化不依赖候选绑定；两个空标识不会伪装成配置可用',()=>{
 const state={configuration_id:null,flow_digest:null,available:false}
 const preview={plan:{configuration:state,items:[]}} as unknown as SharedPreview
 expect(previewConfigurationChanged(preview,{configuration:{...state,available:true}} as Parameters<typeof previewConfigurationChanged>[1])).toBe(true)
 expect(previewConfigurationChanged(preview,{configuration:state} as Parameters<typeof previewConfigurationChanged>[1])).toBe(false)
 expect(previewConfigurationChanged(preview,{configuration:{...state,configuration_id:'installed'}} as Parameters<typeof previewConfigurationChanged>[1])).toBe(true)
})

import {expect,it} from 'vitest'
import {previewSelections,previewConfigurationChanged,isUsageBlocker} from './shared-preview-model'
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

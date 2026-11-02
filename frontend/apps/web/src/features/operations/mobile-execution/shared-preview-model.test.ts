import {expect,it} from 'vitest'
import {previewSelections} from './shared-preview-model'
import type {SharedPreview} from './api'
it('没有旧选中项的目标也会加入本次设备选择',()=>{
 const items=[{target_id:'target',selected:null},{target_id:'other',selected:{device_id:'other-phone'}}] as SharedPreview['plan']['items']
 expect(previewSelections(items,'target','new-phone')).toEqual([{target_id:'other',device_id:'other-phone'},{target_id:'target',device_id:'new-phone'}])
 expect(previewSelections(items,'other','replacement')).toEqual([{target_id:'other',device_id:'replacement'}])
})

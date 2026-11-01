import { expect, it } from 'vitest'
import { policyClock, policyMinute, policyWeekdays, newPolicyWindow } from './shared-policy-time'
it('中文星期保留ISO周日编号', () => {
 expect(policyWeekdays.map(d=>d.label)).toEqual(['周一','周二','周三','周四','周五','周六','周日'])
 expect(policyWeekdays[6].value).toBe(7)
})
it('全天、跨午夜及空输入正确转换', () => {
 expect(policyMinute('00:00',false)).toBe(0)
 expect(policyMinute('00:00',true)).toBe(1440)
 expect(policyClock(1440)).toBe('00:00')
 expect(policyMinute('23:30',false)).toBe(1410)
 expect(policyMinute('01:15',true)).toBe(75)
 for(const v of ['', '24:00', '12:60', '9:00'])expect(policyMinute(v,false)).toBeUndefined()
 expect(policyClock(undefined)).toBe('')
 const first=newPolicyWindow();first.weekdays.push(7)
 expect(newPolicyWindow()).toEqual({weekdays:[1,2,3,4,5],start_minute:540,end_minute:1080})
})

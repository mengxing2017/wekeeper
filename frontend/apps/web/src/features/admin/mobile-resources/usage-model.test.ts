import { expect,it } from 'vitest'
import type { MobileUsage } from './api'
import { openingExpiry,usageRows } from './usage-model'
const row=(id:string,root:string,created:string,approval:MobileUsage['approval']='approved'):MobileUsage=>({id,budget_root:root,supersedes:null,created_at:created,school_id:'school',school_name:'学校',revision:1,approval,state:'active',body:{device_ids:['phone'],targets:[{id:'group',name:'群',search_text:'默认群'}],max_calls:100,expires_at:'2099-05-01T00:00:00Z',auto_targets:false},expires_at:'2099-05-01T00:00:00Z',confirmed_until:'2099-05-01T00:00:00Z',delivery:'available',progress:'已关联',events:[],issues:[]})
it('续期和重新申请只在主列表显示最新版本，保留原批准版本的历史',()=>{
 const old=row('old','family','2026-11-01T00:00:00Z')
 const next={...row('next','family','2026-11-02T00:00:00Z','pending'),supersedes:'old'}
 const unrelated=row('other','separate','2026-11-03T00:00:00Z')
 const result=usageRows([old,unrelated,next])
 expect(result.map(r=>r.id)).toEqual(['other','next'])
 expect(result[1].history).toEqual([old])
 expect(result[1].approval).toBe('pending')
})
it('续期默认日期晚于原期限，使用本地时间字段而不是直接截取 UTC',()=>{
 const now=new Date(2026,10,3,10,15).getTime()
 const original=new Date(2027,0,10,10,15).toISOString()
 const next=new Date(openingExpiry(original,now))
 expect(next.getTime()).toBeGreaterThan(Date.parse(original))
 expect(next.getHours()).toBe(10)
 expect(new Date(openingExpiry(undefined,now)).getTime()).toBeGreaterThan(now)
})

it('临近一年期限时续期默认值仍在服务端允许的 366 天内',()=>{
 const now=new Date(2026,10,3,10,15,23,123).getTime()
 const old=now+360*86400000
 const next=Date.parse(openingExpiry(new Date(old).toISOString(),now))
 expect(next).toBeGreaterThan(old)
 expect(next).toBeLessThan(now+366*86400000)
})

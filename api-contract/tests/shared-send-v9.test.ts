import {expect,it} from 'vitest'
import {createHash} from 'node:crypto'
import f from '../fixtures/shared-mobile-send-v9.json'
import {createSharedSendV2Client,SHARED_SEND_SCHEMA} from '../src/modules/mobile-execution/shared-send/v2'
import {createClient} from '../src/http/client'
function canonicalJson(v:unknown):string {if(v===null||typeof v!=='object')return JSON.stringify(v);if(Array.isArray(v))return '['+v.map(canonicalJson).join(',')+']';const r=v as Record<string,unknown>;return '{'+Object.keys(r).sort().map(k=>JSON.stringify(k)+':'+canonicalJson(r[k])).join(',')+'}'}
it('v9 preserves immutable wire digests while business requests contain no technical delivery fields',async()=>{
 const hash=(v:unknown)=>createHash('sha256').update(canonicalJson(v)).digest('hex')
 expect(hash(f.flow)).toBe(f.flow_digest);expect(hash(f.event)).toBe(f.receipt.body_hash)
 expect(f.event.schema).toBe(SHARED_SEND_SCHEMA)
 const calls:Array<{url:string;body:unknown;key:string|undefined}>=[]
 const client=createClient(async r=>{calls.push({url:r.path,body:r.body?JSON.parse(r.body as string):undefined,key:r.headers['Idempotency-Key']});return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'mobile-usages',id:'new',attributes:{items:[]}}}}})
 const business=createSharedSendV2Client(client)
 await business.usageConfirm('school',{preview_id:'summary',digest:'summary-hash'},'stable-key')
 await business.usageAction('school','usage','pause',{expected_revision:2,reason:'暂停使用'},'pause-key')
 await business.usageIssue('school','usage','无法找到群','issue-key')
 await business.taskPreview('school',{target_ids:['target'],message:'  原样文字\n',between_targets_seconds:30},'preview-key')
 expect(calls[0].body).toEqual({data:{type:'mobile-usages',attributes:{preview_id:'summary',digest:'summary-hash'}}})
 expect(calls[1].body).toEqual({data:{type:'mobile-usages',attributes:{expected_revision:2,reason:'暂停使用'}}})
 expect(calls[2].body).toEqual({data:{type:'mobile-usage-issues',attributes:{description:'无法找到群'}}})
 expect(calls[3].body).toEqual({data:{type:'mobile-usage-task-previews',attributes:{target_ids:['target'],message:'  原样文字\n',between_targets_seconds:30}}})
 expect(business).not.toHaveProperty('manage');expect(business).not.toHaveProperty('upload')
})

it('business input fixture has only business selection values',async()=>{
 const {default:input}=await import('../fixtures/mobile-usage-input.json')
 const calls:unknown[]=[]
 const client=createSharedSendV2Client({requestResource:async(path,...args)=>{calls.push([path,...args]);return {} as never}})
 await client.usagePreview('school',input,'key')
 expect(calls[0]).toEqual(['/api/v1/schools/school/mobile-execution/usage-previews','POST',input,undefined,{'Idempotency-Key':'key'}])
 expect(Object.keys(input).sort()).toEqual(['auto_targets','device_ids','expires_at','max_calls','targets'])
})

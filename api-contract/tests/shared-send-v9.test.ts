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

it('business scheme has its own authenticated platform endpoint, without technical stages',async()=>{
 const {default:input}=await import('../fixtures/mobile-business-scheme.json')
 const calls:unknown[]=[]
 const client=createSharedSendV2Client({requestResource:async(path,...args)=>{calls.push([path,...args]);return {} as never}})
 await client.saveBusinessScheme(input,'scheme-key')
 expect(calls[0]).toEqual(['/api/v1/platform/mobile-execution/business-schemes','POST',input,undefined,{'Idempotency-Key':'scheme-key'}])
 expect(input).not.toHaveProperty('allowed_stages');expect(input).not.toHaveProperty('release_id')
})

it('opening summary exposes the actual phone locator without requiring default locator revision',async()=>{
 const {default:fixture}=await import('../fixtures/mobile-usage-locator.json')
 const locator:import('../src/modules/mobile-execution/shared-send/v2').UsageLocator={...fixture,search_source:'override'}
 expect(locator.search_text).toBe('手机专用家长群')
 expect(locator.target_locator_revision).toBeNull()
 expect(locator.binding_locator_revision).toBe(2)
})


it('Rust and TS share the execution observation contract and lightweight delegation endpoint',async()=>{
 const {default:fixture}=await import('../fixtures/shared-execution-status.json')
 const wire:import('../src/modules/mobile-execution/shared-send/v2').SharedExecutionStatus={...fixture,phase:'waiting'}
 expect(wire.conditions[0].code).toBe('runtime_stale')
 expect(wire.device_state_observed_at).toBeNull();expect(wire.consent_observed_at).not.toBeNull()
 const calls:unknown[]=[]
 const client=createSharedSendV2Client({requestResource:async(...args)=>{calls.push(args);return {} as never}})
 await client.delegationContext()
 expect(calls).toEqual([['/api/v9/device-execution/send-delegation-context']])
})

it('device inbox encodes cursors and receipt resources without creating send claims',async()=>{
 const {createSharedTaskInboxClient}=await import('../src/modules/mobile-execution/shared-send/v2')
 const {default:fixture}=await import('../fixtures/shared-task-inbox.json')
 const calls:Array<{path:string;body:unknown;key:string|undefined}>=[]
 const client=createClient(async r=>{calls.push({path:r.path,body:r.body?JSON.parse(r.body as string):null,key:r.headers['Idempotency-Key']});return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:r.method==='GET'?'mobile-shared-task-inboxes':'mobile-shared-task-receipts',id:fixture.id,attributes:Object.fromEntries(Object.entries(fixture).filter(([key])=>key!=='id'))}}}})
 const inbox=createSharedTaskInboxClient(client)
 const value=await inbox.inbox(50,'{"revision":2}')
 expect(value.items[0].target_name).toBe('合成测试群')
 expect(calls[0].path).toContain('cursor=%7B%22revision%22%3A2%7D')
 const input={receipt_id:fixture.id,queue_revision:2,items:[{attempt_id:fixture.items[0].attempt_id,assignment_revision:1 as const}]}
 await inbox.receipt(input,'fixed-receipt')
 expect(calls[1].body).toEqual({data:{type:'mobile-shared-task-receipts',attributes:input}})
 expect(calls[1].key).toBe('fixed-receipt')
 expect(calls.some(c=>c.path.includes('send-claims'))).toBe(false)
 expect(fixture.items[0]).not.toHaveProperty('message')
})

it('inspection start is an authenticated execution observation with the exact lease identity',async()=>{
 const calls:Array<{path:string;body:unknown}>=[]
 const client=createSharedSendV2Client(createClient(async r=>{calls.push({path:r.path,body:r.body?JSON.parse(r.body as string):null});return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'mobile-shared-inspection-starts',id:f.inspection_start.inspection_id,attributes:{state:'started',dependencies_digest:f.inspection_start.dependencies_digest}}}}}))
 await client.inspectionStarted(f.inspection_start,'start-observation')
 expect(calls[0]).toEqual({path:'/api/v9/device-execution/send-inspection-starts',body:{data:{type:'mobile-shared-inspection-starts',attributes:f.inspection_start}}})
})

it('screen runtime separates sleep from secure lock and includes bounded preparation diagnostics',async()=>{
 const {default:fixture}=await import('../fixtures/shared-screen-runtime.json')
 const report:import('../src/modules/mobile-execution/shared-send/v2').SharedRuntimeReport={...fixture,schema:SHARED_SEND_SCHEMA}
 expect(report.conditions.interactive).toBe(false)
 expect(report.conditions.device_locked).toBe(false)
 expect(report.preparation?.automatic_wake_enabled).toBe(true)
 expect(report.preparation?.retry_after_ms).toBe(0)
})

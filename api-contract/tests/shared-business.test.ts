import {expect,it} from 'vitest'
import {createSharedSendV2Client} from '../src/modules/mobile-execution/shared-send/v2'
import {createClient} from '../src/http/client'
it('unit schemes and task previews use explicit scopes without opening a resource range',async()=>{
 const {default:input}=await import('../fixtures/mobile-business-scheme.json')
 const calls:unknown[]=[]
 const business=createSharedSendV2Client({requestResource:async(path,...args)=>{calls.push([path,...args]);return {} as never}})
 await business.businessScheme('unit/a')
 await business.saveBusinessScheme(input,'save','unit/a')
 await business.inheritBusinessScheme('unit/a',{expected_revision:1,reason:'默认',confirmed:true},'inherit')
 await business.taskPreview({business_unit_id:'unit/a',target_ids:['target'],message:'  原样文字\n',between_targets_seconds:30},'preview')
 expect(calls[0]).toEqual(['/api/v1/mobile-execution/business-schemes/units/unit%2Fa'])
 expect(calls[1]).toEqual(['/api/v1/mobile-execution/business-schemes/units/unit%2Fa/versions','POST',input,undefined,{'Idempotency-Key':'save'}])
 expect((calls[2] as unknown[])[0]).toBe('/api/v1/mobile-execution/business-schemes/units/unit%2Fa/inheritances')
 expect((calls[3] as unknown[])[0]).toBe('/api/v1/mobile-execution/shared-v2-task-previews')
 expect(business).not.toHaveProperty('usageConfirm');expect(business).not.toHaveProperty('usagePreview')
})

it('Rust and TS share observations without native hosting fields',async()=>{
 const {default:fixture}=await import('../fixtures/shared-execution-status.json')
 const wire:import('../src/modules/mobile-execution/shared-send/v2').SharedExecutionStatus={...fixture,phase:'waiting'}
 expect(wire).not.toHaveProperty('consent_observed_at')
 expect(createSharedSendV2Client({requestResource:async()=>({}) as never})).not.toHaveProperty('delegationContext')
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

it('optional environment observations use a separate closed protocol',async()=>{
 const {default:fixture}=await import('../fixtures/device-environment.json')
 const {durableExecutionApi}=await import('../src/modules/mobile-execution/shared-send/v2')
 const report:import('../src/modules/mobile-execution/shared-send/v2').DeviceEnvironmentObservation={...fixture,schema:'device_environment/2',applications:{wecom:'unknown',wechat:'unknown'},interactive:'no',keyguard_locked:'no',device_locked:'no',keyguard_secure:'unknown',accessibility:'yes',notification:'unknown'}
 const calls:unknown[]=[]
 const client=createClient(async r=>{calls.push({path:r.path,body:JSON.parse(r.body as string)});return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'mobile-device-environments',id:report.observation_id,attributes:{recorded:true}}}}})
 await durableExecutionApi(client).observeEnvironment(report,'observation')
 expect(calls).toEqual([{path:'/api/v10/device-execution/environment-observations',body:{data:{type:'mobile-device-environments',attributes:report}}}])
 expect(report).not.toHaveProperty('preparation');expect(report).not.toHaveProperty('paused')
})

it('configuration status includes platform source and observation separately from phone runtime',async()=>{
 const {default:fixture}=await import('../fixtures/shared-configuration-status.json')
 const status:import('../src/modules/mobile-execution/shared-send/v2').SharedConfigurationStatus={...fixture,capability:'wecom.group_text',source:'platform_runtime_configuration'}
 expect(status.configuration_id).toBeNull();expect(status.available).toBe(false)
 expect(status.issue).toEqual({code:'configuration_missing',path:'flow'})
 expect(status.observed_at).toBe('2026-11-04T04:10:09Z')
 expect(status).not.toHaveProperty('flow')
})

it('dispatch feedback exposes the authorized pool and automatic assignment history',async()=>{
 const {default:f}=await import('../fixtures/shared-dispatch.json')
 const snapshot:import('../src/modules/mobile-execution/shared-send/v2').SharedDispatchSnapshot={...f,state:'assigned',assignments:f.assignments.map(a=>({...a,source:'automatic'}))}
 expect(snapshot).toEqual(f)
 expect(snapshot.assignments[0].from_device_name).toBe('手机 A')
 expect(snapshot.assignments[0].to_device_name).toBe('手机 B')
})

it('task lists expose canonical outcomes without loading individual task details',async()=>{
 const {default:cases}=await import('../fixtures/shared-task-status.json')
 const tasks=cases.map(c=>({id:c.name,intent_id:'intent',revision:1,stopped:c.stopped,created_at:'2026-11-08',status:c.status}))
 const client=createClient(async()=>({status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'mobile-shared-tasks',id:'global',attributes:{tasks,drafts:[],readiness:[],pool:[],inspections:[]}}}}))
 const overview=await createSharedSendV2Client(client).overview()
 const statuses:Array<import('../src/modules/mobile-execution/shared-send/v2').SharedTaskStatus>=['completed','completed','stopped','dispatch_failed','dispatching','confirmation_expired','needs_review','completed']
 expect(overview.tasks.map(t=>t.status)).toEqual(statuses)
 expect(overview.tasks.every(t=>!t.items)).toBe(true)
})

it('unavailable is an uncalled attempt result without native state',async()=>{
 const {default:fixture}=await import('../fixtures/device-unavailable-completion.json')
 const result:import('../src/modules/mobile-execution/shared-send/v2').ExecutionCompletion={...fixture,outcome:'not_called'}
 expect(result.reason).toBe('device_unavailable')
 expect(result.consumed).toBe(false);expect(result.calling_marked).toBe(false)
 expect(result).not.toHaveProperty('paused')
})

it('scheme writes and task previews use their Rust JSON API resource types',async()=>{
 const {default:input}=await import('../fixtures/mobile-business-scheme.json')
 const calls:Array<{path:string;body:unknown}>=[]
 const client=createClient(async r=>{const type=r.path.includes('business-schemes')?'mobile-business-schemes':'mobile-task-previews';calls.push({path:r.path,body:r.body?JSON.parse(r.body as string):null});return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type,id:'result',attributes:{}}}}})
 const business=createSharedSendV2Client(client)
 await business.saveBusinessScheme(input,'save')
 await business.saveBusinessScheme(input,'unit-save','unit')
 await business.inheritBusinessScheme('unit',{expected_revision:2,reason:'restore',confirmed:true},'inherit')
 const preview={business_unit_id:null,target_ids:['target'],message:'正文',between_targets_seconds:30}
 await business.taskPreview(preview,'preview')
 expect(calls.map(c=>(c.body as {data:{type:string}}).data.type)).toEqual(['mobile-business-schemes','mobile-business-schemes','mobile-business-schemes','mobile-task-previews'])
 expect(calls[3].body).toEqual({data:{type:'mobile-task-previews',attributes:preview}})
})

it('reads built-in business schemes without requiring a saved version',async()=>{
 const {default:fixture}=await import('../fixtures/mobile-builtin-scheme.json')
 const client=createClient(async()=>({status:200,headers:{'content-type':'application/vnd.api+json'},body:fixture}))
 const scheme=await createSharedSendV2Client(client).businessScheme()
 expect(scheme.id).toBe('builtin:default')
 expect(scheme.source).toBe('builtin')
 expect(scheme.revision).toBe(0)
 expect(scheme.business_unit_id).toBeNull()
})

import {expect,it} from 'vitest'
import {createClient} from '../src/http/client'
import {createWecomDistributionClient} from '../src/modules/wecom-distribution/client'
import {createSourceAutomationClient} from '../src/modules/source-access/automation-client'
import {durableExecutionApi} from '../src/modules/mobile-execution/shared-send/v2'
import type {ChannelObservation,ExecutionCapability} from '../src/modules/execution-evidence'
import observation from '../fixtures/channel-execution-observation.json'
import capability from '../fixtures/wecom-execution-capability.json'
import durable from '../fixtures/durable-execution.json'

it('preserves external confirmation and result provenance without treating creation as sent',async()=>{
 const paths:string[]=[]
 const transport=createClient(async r=>{
  paths.push(r.path)
  return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'source-automation-tasks',id:'result',attributes:{items:[observation],next_cursor:observation.id}}}}
 })
 const automatic=createSourceAutomationClient(transport)
 const result=await automatic.observations('task-1',50)
 const typed:ChannelObservation=result.items[0]
 const mode:ExecutionCapability=capability as ExecutionCapability
 expect(paths[0]).toBe('/api/v2/admin/source-automation-tasks/task-1/observations?after=50')
 expect(typed.normalized_result).toBe('pending')
 expect(mode.platform_interaction).toBe('none')
 expect(mode.external_confirmation).toBe('member_required')
 const official=createWecomDistributionClient(createClient(async r=>{
  paths.push(r.path)
  return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:[{type:'broadcast-observations',id:String(observation.id),attributes:Object.fromEntries(Object.entries(observation).filter(([k])=>k!=='id'))}]}}
 }))
 expect((await official.broadcast.observations('workspace','task',2)).items[0].evidence_type).toBe('official_task_created')
 expect(paths[1]).toContain('/observations?page[number]=2')
})

it('includes optional local times in the same durable completion identity',async()=>{
 let payload:unknown
 const client=durableExecutionApi(createClient(async r=>{
  payload=JSON.parse(r.body as string)
  expect(r.headers['Idempotency-Key']).toBe(durable.completion.result_id)
  return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'execution-completions',id:'receipt',attributes:{recorded:true}}}}
 }))
 const input={...durable.completion,outcome:'observed_sent' as const,device_started_at:'2026-11-10T00:00:00Z',device_finished_at:'2026-11-10T00:00:01Z'}
 await client.complete(input)
 expect(payload).toMatchObject({data:{type:'execution-completions',attributes:input}})
})

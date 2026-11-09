import {describe,it,expect} from 'vitest'
import {createClient} from '../src/http/client'
import {createSourceAutomationClient} from '../src/modules/source-access/automation-client'
import task from '../fixtures/source-automation-task.json'
describe('automatic task contract',()=>{
 it('keeps read filters, task identity and explicit control JSON:API types',async()=>{
  const calls:{path:string;body?:string|ArrayBuffer}[]=[]
  const client=createSourceAutomationClient(createClient(async r=>{calls.push(r);return {status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:{type:'source-automation-tasks',id:'result',attributes:r.method==='GET'?{items:[task],has_more:false}:{accepted:true}}}}}))
  const list=await client.list({route_id:'route-1',state:'running',page:2})
  expect(list.items[0].counts.total).toBe(3)
  expect(calls[0].path).toContain('route_id=route-1&state=running&page=2')
  const control={expected_revision:1,idempotency_key:'00000000-0000-0000-0000-000000000001',reason:'停止后续提交'}
  await client.stop(task.id,control)
  expect(JSON.parse(calls[1].body as string).data).toMatchObject({type:'source-automation-task-stops',attributes:control})
  await client.retry(task.id,control)
  expect(JSON.parse(calls[2].body as string).data.type).toBe('source-automation-task-retry-requests')
  await client.savePolicy('route-1',0,'resend_new_version','明确链路授权')
  expect(JSON.parse(calls[3].body as string).data.type).toBe('source-route-version-policies')
 })
})

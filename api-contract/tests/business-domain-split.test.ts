import { afterEach, expect, it, vi } from 'vitest'
import { broadcast, officialSenders, distributionPolicies } from '../src/modules/wecom-distribution'
afterEach(() => vi.unstubAllGlobals())
function mockResponse(type:string,attributes:unknown) {const fetch=vi.fn().mockImplementation(async()=>Response.json({data:{type,id:'id',attributes}},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);return fetch}
it('binds server preview digest and creation key without permitting implicit confirmation',async()=>{
 const fetch=mockResponse('broadcast-tasks',{})
 await broadcast.save('school',{content_source:'materials',materials:[{namespace:'test.other',external_organization_id:'1',external_content_id:'1',version:'v1'}],group_ids:['g']},undefined,'same-draft')
 expect(fetch.mock.calls[0][1].headers['Idempotency-Key']).toBe('same-draft')
 expect(JSON.parse(fetch.mock.calls[0][1].body).data.attributes).not.toHaveProperty('confirmed')
 await broadcast.preview('school','task')
 expect(fetch.mock.calls[1][0]).toBe('/api/v1/wecom-distribution/workspaces/school/broadcast-tasks/task/preview')
 await broadcast.action('school','task','confirm','one',{revision:2,preview_digest:'frozen'})
 expect(JSON.parse(fetch.mock.calls[2][1].body).data.attributes).toEqual({revision:2,preview_digest:'frozen'})
})
it('separates platform sender governance from school summaries and phone accounts',async()=>{
 const fetch=vi.fn().mockImplementation(async()=>Response.json({data:[]},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch)
 await officialSenders.list('school');await officialSenders.list(undefined)
 expect(fetch.mock.calls[0][0]).toContain('/wecom-distribution/workspaces/school/wecom-senders')
 expect(fetch.mock.calls[1][0]).toContain('/platform/wecom-senders')
 mockResponse('wecom-sender-qualifications',{updated:true})
 await officialSenders.qualify('school','member',{enabled:false,revision:3})
})
it('platform policy updates carry a revision and cannot create an external source',async()=>{
 const fetch=mockResponse('distribution-policy-updates',{updated:true})
 await distributionPolicies.source({namespace:'builtin.forum',enabled:false,max_pending:10,revision:1})
 expect(JSON.parse(fetch.mock.calls[0][1].body).data.attributes).toEqual({namespace:'builtin.forum',enabled:false,max_pending:10,revision:1})
})
it('official client is transport-injected and keeps platform credentials out of another client',async()=>{
 const {createClient}=await import('../src/http/client')
 const {createWecomDistributionClient}=await import('../src/modules/wecom-distribution/client')
 const transport=vi.fn().mockResolvedValue({status:200,headers:{'content-type':'application/vnd.api+json'},body:{data:[]}})
 const client=createClient(transport);client.setCsrf('own-platform-csrf')
 await createWecomDistributionClient(client).officialSenders.list('school')
 expect(transport.mock.calls[0][0].path).toContain('/wecom-distribution/workspaces/school/wecom-senders')
 expect(transport.mock.calls[0][0].headers).not.toHaveProperty('Authorization')
})

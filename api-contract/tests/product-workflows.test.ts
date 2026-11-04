import {afterEach,expect,it,vi} from 'vitest'
import {product,broadcast} from '../src/modules/business'
import fixture from '../fixtures/product-workflows.json'
import type {ResourceInput,TemplateInput,UsageInput,MemberPolicyInput,RequestInput,RequestDecision,Confirmation} from '../src/modules/product-workflows/client'
afterEach(()=>vi.unstubAllGlobals())
function mock(){const fetch=vi.fn().mockImplementation(async()=>Response.json({data:{type:'business-results',id:'result',attributes:{state:'pending'}}},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);return fetch}
it('uses the same typed Rust business inputs for all preview and confirm operations',async()=>{
 const fetch=mock()
 const operations:[()=>Promise<unknown>,string,string,unknown][]=[
 [()=>product.resources.preview('school',fixture.resource as ResourceInput),'/schools/school/resource-previews','resource-previews',fixture.resource],
 [()=>product.templates.preview('school',fixture.template as TemplateInput),'/schools/school/template-previews','template-previews',fixture.template],
 [()=>product.officialUsage.preview('school',fixture.usage as UsageInput),'/schools/school/official-usage-previews','official-usage-previews',fixture.usage],
 [()=>product.memberPolicies.preview(fixture.policy as MemberPolicyInput),'/platform/member-policy-previews','member-policy-previews',fixture.policy],
 [()=>product.requests.channel('school',fixture.request as RequestInput),'/schools/school/broadcast-access-requests','broadcast-access-requests',fixture.request],
 [()=>product.requests.decide('request',fixture.decision as RequestDecision),'/platform/business-requests/request/decisions','business-requests',fixture.decision],
 [()=>product.resources.confirm('school',fixture.confirmation as Confirmation),'/schools/school/resource-publications','resource-publications',fixture.confirmation],
 [()=>product.templates.confirm('school',fixture.confirmation as Confirmation),'/schools/school/template-publications','template-publications',fixture.confirmation],
 [()=>product.officialUsage.confirm('school',fixture.confirmation as Confirmation),'/schools/school/official-usages','official-usages',fixture.confirmation],
 [()=>product.memberPolicies.confirm(fixture.confirmation as Confirmation),'/platform/member-policies','member-policies',fixture.confirmation],
 ]
 for(const [run,path,type,attributes] of operations){await run();const [url,options]=fetch.mock.lastCall!;expect(url).toBe(`/api/v1${path}`);const {id,...rest}=attributes as Record<string,unknown>;expect(JSON.parse(options.body)).toEqual({data:{type,...(id?{id}:{}),attributes:rest}})}
})
it('saves a recoverable no-target draft and preview in one request without implicit send',async()=>{
 const fetch=mock()
 await broadcast.savePreview('school',{content_source:'text',text_content:'通知',group_ids:[]},undefined,'same-intent')
 expect(fetch.mock.calls).toHaveLength(1)
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/schools/school/broadcast-task-previews')
 expect(fetch.mock.calls[0][1].headers['Idempotency-Key']).toBe('same-intent')
 expect(JSON.parse(fetch.mock.calls[0][1].body).data.attributes).toEqual({content_source:'text',text_content:'通知',group_ids:[]})
})
it('candidate queries stay within school/task/batch and contain no company time range',async()=>{
 const fetch=mock();await broadcast.evidenceCandidates('school','task','batch')
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/schools/school/broadcast-tasks/task/evidence-candidates?batch_id=batch')
})
it('consumers always use published resources and cannot switch into management view',async()=>{
 const fetch=mock();await product.resources.list('school')
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/schools/school/published-resources')
})

it('channel pause is a revisioned platform business action, without delivery approval inputs',async()=>{
 const fetch=mock();const input=fixture.channel_control
 await product.officialChannelControl(input)
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/platform/official-channel-controls')
 expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'official-channel-controls',attributes:input}})
})

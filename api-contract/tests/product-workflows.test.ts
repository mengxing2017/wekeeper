import {afterEach,expect,it,vi} from 'vitest'
import {product} from '../src/modules/product-workflows'
import {broadcast} from '../src/modules/wecom-distribution'
import fixture from '../fixtures/product-workflows.json'
import type {UsageInput,MemberPolicyInput,RequestInput,RequestDecision,Confirmation} from '../src/modules/product-workflows/client'
afterEach(()=>vi.unstubAllGlobals())
function mock(){const fetch=vi.fn().mockImplementation(async()=>Response.json({data:{type:'business-results',id:'result',attributes:{state:'pending'}}},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch);return fetch}
it('uses the same typed Rust business inputs for all preview and confirm operations',async()=>{
 const fetch=mock()
 const operations:[()=>Promise<unknown>,string,string,unknown][]=[
 [()=>product.officialUsage.preview(fixture.usage as UsageInput),'/wecom-distribution/official-usage-previews','official-usage-previews',fixture.usage],
 [()=>product.memberPolicies.preview(fixture.policy as MemberPolicyInput),'/platform/member-policy-previews','member-policy-previews',fixture.policy],
 [()=>product.requests.channel(fixture.request as RequestInput),'/wecom-distribution/broadcast-access-requests','broadcast-access-requests',fixture.request],
 [()=>product.requests.decide('request',fixture.decision as RequestDecision),'/platform/business-requests/request/decisions','business-requests',fixture.decision],
 [()=>product.officialUsage.confirm(fixture.confirmation as Confirmation),'/wecom-distribution/official-usages','official-usages',fixture.confirmation],
 [()=>product.memberPolicies.confirm(fixture.confirmation as Confirmation),'/platform/member-policies','member-policies',fixture.confirmation],
 ]
 for(const [run,path,type,attributes] of operations){await run();const [url,options]=fetch.mock.lastCall!;expect(url).toBe(`/api/v1${path}`);const {id,...rest}=attributes as Record<string,unknown>;expect(JSON.parse(options.body)).toEqual({data:{type,...(id?{id}:{}),attributes:rest}})}
})
it('saves a recoverable no-target draft and preview in one request without implicit send',async()=>{
 const fetch=mock()
 await broadcast.savePreview({content_source:'text',text_content:'通知',group_ids:[]},undefined,'same-intent')
 expect(fetch.mock.calls).toHaveLength(1)
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/wecom-distribution/broadcast-task-previews')
 expect(fetch.mock.calls[0][1].headers['Idempotency-Key']).toBe('same-intent')
 expect(JSON.parse(fetch.mock.calls[0][1].body).data.attributes).toEqual({content_source:'text',text_content:'通知',group_ids:[]})
})
it('candidate queries stay within task/batch and contain no company time range',async()=>{
 const fetch=mock();await broadcast.evidenceCandidates('task','batch')
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/wecom-distribution/broadcast-tasks/task/evidence-candidates?batch_id=batch')
})
it('channel pause is a revisioned platform business action, without delivery approval inputs',async()=>{
 const fetch=mock();const input=fixture.channel_control
 await product.officialChannelControl(input)
 expect(fetch.mock.calls[0][0]).toBe('/api/v1/platform/official-channel-controls')
 expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'official-channel-controls',attributes:input}})
})

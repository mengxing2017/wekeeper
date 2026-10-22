import {afterEach,expect,it,vi} from 'vitest'
import {broadcast} from '../src/modules/business'
import task from '../fixtures/broadcast-task.json'
import target from '../fixtures/broadcast-target.json'
afterEach(()=>vi.unstubAllGlobals())
it('decodes Rust task and target status facts independently',async()=>{
 const fetch=vi.fn().mockResolvedValueOnce(Response.json(task,{headers:{'Content-Type':'application/vnd.api+json'}})).mockResolvedValueOnce(Response.json({data:[target.data],meta:{page:{number:1,size:100,has_more:false}}},{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch)
 expect(await broadcast.task('school','task')).toMatchObject({total:2,sent:1,pending:1})
 expect((await broadcast.targets('school','task')).items[0]).toMatchObject({member_status:2,official_status:null})
})
it('confirms the exact revision with a stable idempotency key and no automatic retry',async()=>{
 const fetch=vi.fn().mockRejectedValue(new TypeError('response lost'));vi.stubGlobal('fetch',fetch)
 await expect(broadcast.action('school','task','confirm','one-invocation',{revision:4})).rejects.toThrow('网络中断')
 expect(fetch).toHaveBeenCalledTimes(1)
 expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({data:{type:'broadcast-tasks',attributes:{revision:4}}})
 expect(fetch.mock.calls[0][1].headers['Idempotency-Key']).toBe('one-invocation')
})
it('draft editing uses PATCH and preserves explicit selected groups',async()=>{
 const fetch=vi.fn().mockResolvedValue(Response.json(task,{headers:{'Content-Type':'application/vnd.api+json'}}));vi.stubGlobal('fetch',fetch)
 const input={template_version_id:'v',post_ids:['p'],group_ids:['g1','g2'],revision:2}
 await broadcast.save('school',input,'draft')
 expect(fetch.mock.calls[0][1].method).toBe('PATCH')
 expect(JSON.parse(fetch.mock.calls[0][1].body).data.attributes).toEqual(input)
})

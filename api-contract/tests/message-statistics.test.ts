import {afterEach,expect,it,vi} from 'vitest'
import {messageStatistics} from '../src/modules/message-statistics'
import fixture from '../fixtures/message-statistics.json'
afterEach(()=>vi.unstubAllGlobals())
it('uses JSON API statistics resource identities, cancellation and exact count strings',async()=>{
 const {id,...attributes}=fixture
 const fetchMock=vi.fn(async(_url:string,_init:{body:string})=>Response.json({data:{type:'message-statistics-queries',id,attributes}},{headers:{'Content-Type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 const data=await messageStatistics.create({channel:'mobile',client_kind:'wechat'})
 expect(data.summary.targets.total).toBe('9007199254740993')
 const body=JSON.parse(fetchMock.mock.calls[0][1].body)
 expect(body.data.type).toBe('message-statistics-queries')
 expect(body.data.attributes).toEqual({channel:'mobile',client_kind:'wechat'})
 await messageStatistics.details(fixture.id,{object:'responsibility',reason_code:'a b',attention:true,page:2})
 expect(fetchMock.mock.calls[1][0]).toContain('object=responsibility&reason_code=a+b&attention=true&page=2')
 await messageStatistics.export(fixture.id)
 expect(JSON.parse(fetchMock.mock.calls[2][1].body).data.type).toBe('message-statistics-exports')
 const cancelled=new AbortController();cancelled.abort()
 await expect(messageStatistics.get(fixture.id,cancelled.signal)).rejects.toMatchObject({code:'cancelled'})
})

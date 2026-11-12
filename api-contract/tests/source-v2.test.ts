import {afterEach,describe,it,expect,vi} from 'vitest'
import {createHash,createHmac} from 'node:crypto'
import {readFileSync} from 'node:fs'
import {resourceType,writeDocument} from '../src/http/protocol'
import {sourceAccess} from '../src/modules/source-access'
import secretFixture from '../fixtures/source-credential-secret.json'
import unitFixture from '../fixtures/source-unit-management.json'
afterEach(()=>vi.unstubAllGlobals())
const fixture=JSON.parse(readFileSync(new URL('../fixtures/source-v2.json',import.meta.url),'utf8'))
describe('published source-v2 bytes',()=>{
 it('matches independently generated UTF-8, empty GET and check vectors',()=>{
  for(const v of fixture.vectors){
   const digest=createHash('sha256').update(v.body,'utf8').digest('hex')
   expect(digest).toBe(v.digest)
   const text=['2',v.key_id,v.method,v.path,v.timestamp,v.nonce,v.identity,digest].join('\n')
   expect(text).toBe(v.canonical)
   expect(createHmac('sha256',Buffer.from(v.secret,'base64url')).update(text,'utf8').digest('hex')).toBe(v.signature)
   expect(createHmac('sha256',Buffer.from(v.secret,'base64url')).update(text+'\n').digest('hex')).not.toBe(v.signature)
  }
 })
 it('selects management action types without changing attributes',()=>{
expect(writeDocument('/api/v2/admin/source-credentials/key',{valid_until:null}).data.attributes).toEqual({valid_until:null});
expect(resourceType('/api/v2/admin/source-credentials/key')).toBe('source-credentials'); expect(writeDocument('/api/v2/admin/source-credentials/key',{revision:1}).data.attributes).toEqual({revision:1})
 })
})

it('reveals a credential through an explicit POST and consumes the Rust fixture',async()=>{
 const fetchMock=vi.fn().mockImplementation(async()=>Response.json(secretFixture,{headers:{'content-type':'application/vnd.api+json','cache-control':'no-store'}}))
 vi.stubGlobal('fetch',fetchMock)
 for(let n=0;n<2;n++)expect(await sourceAccess.revealSecret('fixture-key')).toEqual({id:'fixture-key',...secretFixture.data.attributes})
 expect(fetchMock).toHaveBeenCalledTimes(2)
 const [url,options]=fetchMock.mock.calls[0]
 expect(url).toBe('/api/v2/admin/source-credentials/fixture-key/secret-reveals')
 expect(options.method).toBe('POST')
 expect(JSON.parse(options.body)).toEqual({data:{type:'source-credential-secret-reveals',attributes:{}}})
})

it('filters child routes by credential ID without requiring a second caller ID',async()=>{
 const fetchMock=vi.fn().mockResolvedValue(Response.json({data:[],meta:{has_more:false}},{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 await sourceAccess.list('source-routes',1,'','','key-1')
 const query=new URL(fetchMock.mock.calls[0][0],'https://keeper.example.com').searchParams
 expect(query.get('key_id')).toBe('key-1')
 expect(query.has('source_id')).toBe(false)
})

it('sends the unit name in the route management request',async()=>{
 const fetchMock=vi.fn().mockResolvedValue(Response.json(unitFixture.response,{headers:{'content-type':'application/vnd.api+json'}}))
 vi.stubGlobal('fetch',fetchMock)
 expect(await sourceAccess.save('source-routes',unitFixture.request)).toEqual(unitFixture.result)
 const [url,options]=fetchMock.mock.calls[0]
 expect(url).toBe('/api/v2/admin/source-routes')
 expect(JSON.parse(options.body)).toEqual({data:{type:'source-routes',attributes:unitFixture.request}})
})

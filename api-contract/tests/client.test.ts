import { afterEach, describe, expect, it, vi } from 'vitest'
import fixture from '../fixtures/system.json'
import { ApiError, getJson } from '../src/http'
import { workspace } from '../src/modules/business'
import { getSystemInfo, type SystemInfo } from '../src/modules/system'

function json(value: unknown, init: ResponseInit = {}) { return Response.json(value, {...init, headers:{'Content-Type':'application/vnd.api+json',...init.headers}}) }

const typedFixture: SystemInfo = {id:'wekeeper', application: 'wekeeper', version: '0.1.0', stage: 'business'}

afterEach(() => vi.unstubAllGlobals())

describe('shared HTTP contract', () => {
  it('uses the fixture also asserted by the Rust HTTP test', async () => {
    expect(fixture.data.attributes).toEqual({application:'wekeeper',version:'0.1.0',stage:'business'})
    const fetchMock = vi.fn().mockResolvedValue(json(fixture))
    vi.stubGlobal('fetch', fetchMock)
    expect(await getSystemInfo()).toEqual(typedFixture)
    expect(fetchMock).toHaveBeenCalledWith('/api/v1/system', expect.objectContaining({ credentials: 'same-origin' }))
  })

  it('rejects cross-origin paths before making a request', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    await expect(getJson('//example.com/api')).rejects.toThrow('same-origin')
    await expect(getJson('/\\example.com/api')).rejects.toThrow('same-origin')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('preserves structured server errors', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json({ errors: [{ status: '403', code: 'forbidden', title: 'Forbidden', detail: '无权访问' }] }, { status: 403 })))
    await expect(getSystemInfo()).rejects.toMatchObject({ status: 403, code: 'forbidden', message: '无权访问' })
  })

  it('turns proxy HTML into a usable HTTP error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('Bad gateway', { status: 502 })))
    await expect(getSystemInfo()).rejects.toBeInstanceOf(ApiError)
  })
})

it('keeps OAuth selection in a fixed local entry URL', async () => {
  const { wecomOAuthUrl } = await import('../src/modules/business')
  expect(wecomOAuthUrl()).toBe('/api/v1/wecom/oauth/start')
  expect(wecomOAuthUrl({workspace: '00000000-0000-0000-0000-000000000000'})).toBe('/api/v1/wecom/oauth/start?workspace=00000000-0000-0000-0000-000000000000')
})

it('reads the public WeCom status using the backend fixture', async () => {
  const { workspace } = await import('../src/modules/business')
  const fixture = (await import('../fixtures/wecom-status.json')).default
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json(fixture)))
  expect(await workspace.wecomStatus()).toEqual({id:'current',server_configured:false,client_configured:false,oauth_available:false,share_available:false})
})

it('consumes the Cairnkit error fixture serialized by Rust', async () => {
  const fixture = (await import('../fixtures/not-found.json')).default
  const mock = vi.fn().mockResolvedValue(json(fixture,{status:404,headers:{'Content-Type':'application/vnd.api+json'}}))
  vi.stubGlobal('fetch',mock)
  await expect(getJson('/api/v1/missing')).rejects.toMatchObject({status:404,code:'not_found',message:'接口不存在'})
  expect(mock).toHaveBeenCalledWith('/api/v1/missing',expect.objectContaining({headers:expect.objectContaining({Accept:'application/vnd.api+json'})}))
})
it.each([null, {}, {errors:[]}, {errors:[null]}, {errors:[{code:12,detail:{}}]}])('handles malformed gateway errors without throwing TypeError: %j', async failure => {
  vi.stubGlobal('fetch',vi.fn().mockResolvedValue(json(failure,{status:502})))
  await expect(getSystemInfo()).rejects.toMatchObject({status:502,code:'http_error',message:'服务暂时不可用'})
})
it('uses JSON:API title when no detail is available', async () => {
  vi.stubGlobal('fetch',vi.fn().mockResolvedValue(json({errors:[{title:'Unavailable'}]},{status:503})))
  await expect(getSystemInfo()).rejects.toMatchObject({status:503,message:'Unavailable'})
})

it('sends only JSON:API resources, and rejects retired forum writes', async () => {
  const mock = vi.fn().mockImplementation(async () => json({data:null}))
  vi.stubGlobal('fetch', mock)
  await workspace.mutate('s','groups',{id:'g',name:'Group'})
  expect(mock.mock.calls[0][1]).toMatchObject({headers:{'Content-Type':'application/vnd.api+json'},body:JSON.stringify({data:{type:'groups',id:'g',attributes:{name:'Group'}}})})
  await expect(workspace.mutate('s','posts/p/comments',{content:'Reply'})).rejects.toThrow()
  await expect(workspace.mutate('s','templates/t/versions',{body:'Template'})).rejects.toThrow()
  expect(mock).toHaveBeenCalledTimes(1)
})
it.each([{data:{id:'old',title:'Legacy'}},{data:{type:'posts',id:'p',attributes:{id:'conflict'}}},{data:[],meta:{page:{number:1,size:30,has_more:'false'}}},{data:null,errors:[]}])('rejects incompatible success documents rather than supporting two formats: %j',async fixture=>{
  vi.stubGlobal('fetch',vi.fn().mockResolvedValue(json(fixture)))
  await expect(workspace.list('s','groups')).rejects.toMatchObject({code:'invalid_document'})
})
it('exposes authoritative page metadata and empty collections',async()=>{
  vi.stubGlobal('fetch',vi.fn().mockResolvedValue(json({data:[],meta:{page:{number:2,size:30,has_more:false}}})))
  expect(await workspace.list('s','groups','?page[number]=2')).toEqual({items:[],page:{number:2,size:30,has_more:false}})
})
it('rejects a successful application/json response after the protocol cutover',async()=>{
  vi.stubGlobal('fetch',vi.fn().mockResolvedValue(Response.json({data:null})))
  await expect(workspace.mutate('s','groups',{name:'Group'})).rejects.toMatchObject({code:'invalid_document'})
})

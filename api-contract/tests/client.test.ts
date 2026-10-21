import { afterEach, describe, expect, it, vi } from 'vitest'
import fixture from '../fixtures/system.json'
import { ApiError, getJson } from '../src/http'
import { getSystemInfo, type SystemDocument } from '../src/modules/system'

const typedFixture: SystemDocument = {
  data: { application: 'wekeeper', version: '0.1.0', stage: 'business' },
}

afterEach(() => vi.unstubAllGlobals())

describe('shared HTTP contract', () => {
  it('uses the fixture also asserted by the Rust HTTP test', async () => {
    expect(fixture).toEqual(typedFixture)
    const fetchMock = vi.fn().mockResolvedValue(Response.json(typedFixture))
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
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ error: { code: 'forbidden', message: '无权访问' } }, { status: 403 })))
    await expect(getSystemInfo()).rejects.toMatchObject({ status: 403, code: 'forbidden', message: '无权访问' })
  })

  it('turns proxy HTML into a usable HTTP error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('Bad gateway', { status: 502 })))
    await expect(getSystemInfo()).rejects.toBeInstanceOf(ApiError)
  })
})

import postFixture from '../fixtures/post-created.json'
import { workspace, type Post } from '../src/modules/business'
import { setCsrf } from '../src/http'
it('shares a real Rust create-post response and sends CSRF for writes', async () => {
  const typed: Post = {...postFixture, audit_status:'pending', visibility:'visible'}
  expect(typed).toEqual(postFixture)
  const fetchMock=vi.fn().mockResolvedValue(Response.json({data:postFixture}))
  vi.stubGlobal('fetch',fetchMock);setCsrf('test-csrf')
  expect(await workspace.mutate('school','posts',{title:'Campus',content:'Hello',images:[],tags:[]})).toEqual(postFixture)
  expect(fetchMock).toHaveBeenCalledWith('/api/v1/schools/school/posts',expect.objectContaining({method:'POST',headers:expect.objectContaining({'X-CSRF-Token':'test-csrf'})}))
  setCsrf('')
})

it('keeps OAuth selection in a fixed local entry URL', async () => {
  const { wecomOAuthUrl } = await import('../src/modules/business')
  expect(wecomOAuthUrl()).toBe('/api/v1/wecom/oauth/start')
  expect(wecomOAuthUrl({school: '00000000-0000-0000-0000-000000000000', draft: '00000000-0000-0000-0000-000000000001'})).toBe('/api/v1/wecom/oauth/start?school=00000000-0000-0000-0000-000000000000&draft=00000000-0000-0000-0000-000000000001')
})

it('reads the public WeCom status using the backend fixture', async () => {
  const { workspace } = await import('../src/modules/business')
  const fixture = (await import('../fixtures/wecom-status.json')).default
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json(fixture)))
  expect(await workspace.wecomStatus()).toEqual({configured:false,oauth_available:false})
})

it('reads cross-draft related history with the Rust serialization contract', async () => {
  const fixture = (await import('../fixtures/delivery-history.json')).default
  const fetchMock = vi.fn().mockResolvedValue(Response.json(fixture))
  vi.stubGlobal('fetch', fetchMock)
  const history = await workspace.relatedHistory('school', 'old-draft', 2)
  expect(history).toEqual(fixture.data)
  expect(fetchMock).toHaveBeenCalledWith('/api/v1/schools/school/delivery-drafts/old-draft/history?page=2&page_size=30', expect.anything())
})
it('loads a draft by ID without depending on the current list page', async () => {
  const fetchMock = vi.fn().mockResolvedValue(Response.json({data:{id:'old-draft'}}))
  vi.stubGlobal('fetch', fetchMock)
  expect(await workspace.getDraft('school','old-draft')).toEqual({id:'old-draft'})
  expect(fetchMock).toHaveBeenCalledWith('/api/v1/schools/school/delivery-drafts/old-draft', expect.anything())
})

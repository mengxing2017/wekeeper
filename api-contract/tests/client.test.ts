import { afterEach, describe, expect, it, vi } from 'vitest'
import fixture from '../fixtures/system.json'
import { ApiError, getJson } from '../src/http'
import { getSystemInfo, type SystemDocument } from '../src/modules/system'

const typedFixture: SystemDocument = {
  data: { application: 'wekeeper', version: '0.1.0', stage: 'scaffold' },
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

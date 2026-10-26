import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createUniTransport, trustedOrigin } from '../src/app/transport'
import { credentials } from '../src/app/credentials'
const uniMock = {
  getStorageSync: vi.fn(),
  setStorageSync: vi.fn(),
  removeStorageSync: vi.fn(),
  request: vi.fn(),
}
beforeEach(() => {
  vi.stubGlobal('uni', uniMock)
  credentials.set('')
  uniMock.request.mockReset()
})
afterEach(() => vi.unstubAllGlobals())
it('only allows a configured HTTPS origin', () => {
  for (const value of [
    'http://example.com',
    'https://user:secret@example.com',
    'https://example.com/path',
    '//example.com',
  ])
    expect(() => trustedOrigin(value)).toThrow()
  expect(trustedOrigin('https://api.example.com')).toBe('https://api.example.com')
})
it('adds only miniapp credentials and preserves raw bytes and HTTP failure status', async () => {
  credentials.set('token-a')
  const bytes = new Uint8Array([0, 255, 137, 80]).buffer
  uniMock.request.mockImplementation((options) => {
    queueMicrotask(() => {
      options.success({
        statusCode: 403,
        header: { 'Content-Type': 'application/vnd.api+json' },
        data: '{"errors":[{"code":"forbidden"}]}',
      })
      options.complete()
    })
    return { abort: vi.fn() }
  })
  const result = await createUniTransport('https://api.example.com')({
    path: '/api/v1/schools/s/media',
    method: 'POST',
    headers: { 'Content-Type': 'application/octet-stream' },
    body: bytes,
  })
  const call = uniMock.request.mock.calls[0][0]
  expect(call.url).toBe('https://api.example.com/api/v1/schools/s/media')
  expect(call.header).toMatchObject({ Authorization: 'Bearer token-a' })
  expect(call.header).not.toHaveProperty('Cookie')
  expect(call.header).not.toHaveProperty('X-CSRF-Token')
  expect(call.data).toBe(bytes)
  expect(result.status).toBe(403)
})
it('cancels pending requests and rejects late responses after credentials change', async () => {
  let options: Record<string, (result?: unknown) => void> = {}
  const abort = vi.fn()
  uniMock.request.mockImplementation((value) => {
    options = value
    return { abort }
  })
  credentials.set('old')
  const pending = createUniTransport('https://api.example.com')({
    path: '/api/v1/auth/session',
    method: 'GET',
    headers: {},
  })
  credentials.set('new')
  expect(abort).toHaveBeenCalledOnce()
  options.success({ statusCode: 200, header: {}, data: '{}' })
  await expect(pending).rejects.toThrow('会话已更新')
  options.complete()
})
it('invalidates token on 401 but not 403, preserving the response for error decoding', async () => {
  credentials.set('expired')
  uniMock.request.mockImplementation((options) => {
    queueMicrotask(() => {
      options.success({ statusCode: 401, header: {}, data: '{"errors":[{"code":"unauthorized"}]}' })
      options.complete()
    })
    return { abort: vi.fn() }
  })
  expect(
    (
      await createUniTransport('https://api.example.com')({
        path: '/api/v1/auth/session',
        method: 'GET',
        headers: {},
      })
    ).status,
  ).toBe(401)
  expect(credentials.token).toBe('')
  expect(uniMock.removeStorageSync).toHaveBeenCalled()
})

it('decodes structured errors returned by binary media requests', async () => {
  credentials.set('valid')
  uniMock.request.mockImplementation((options) => {
    queueMicrotask(() => {
      options.success({
        statusCode: 403,
        header: {},
        data: new TextEncoder().encode(
          JSON.stringify({ errors: [{ code: 'forbidden', detail: '学校权限已撤销' }] }),
        ).buffer,
      })
      options.complete()
    })
    return { abort: vi.fn() }
  })
  const result = await createUniTransport('https://api.example.com')({
    path: '/api/v1/schools/s/media/m',
    method: 'GET',
    headers: {},
    binary: true,
  })
  expect(result.body).toEqual({ errors: [{ code: 'forbidden', detail: '学校权限已撤销' }] })
  expect(credentials.token).toBe('valid')
})

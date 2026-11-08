import { describe, it, expect, vi } from 'vitest'
import { createClient, type Transport } from '../src/http/client'
import { createMediaClient } from '../src/modules/media'
const response = (body: unknown, status = 200) => ({
  status,
  headers: { 'content-type': 'application/vnd.api+json' },
  body,
})

describe('platform-independent clients', () => {
  it('isolates credentials and preserves raw binary bytes', async () => {
    const a = vi.fn<Transport>().mockResolvedValue(response({ data: null })),
      b = vi.fn<Transport>().mockResolvedValue(response({ data: null }))
    const browser = createClient(a),
      other = createClient(b)
    browser.setCsrf('browser-csrf')
    await browser.requestResource('/api/v1/profile', 'PUT', {
      display_name: 'A',
    })
    const bytes = new Uint8Array([137, 80, 78, 71, 0, 255]).buffer
    await createMediaClient(other).upload('school', bytes)
    expect(a.mock.calls[0][0].headers['X-CSRF-Token']).toBe('browser-csrf')
    expect(b.mock.calls[0][0].headers).not.toHaveProperty('X-CSRF-Token')
    expect(b.mock.calls[0][0].body).toBe(bytes)
    expect(b.mock.calls[0][0].headers['Content-Type']).toBe('application/octet-stream')
  })
  it('rejects cross-school media and hostile paths before sending credentials', async () => {
    const transport = vi.fn<Transport>(),
      client = createClient(transport),
      media = createMediaClient(client)
    expect(() =>
      media.download('a', '/api/v1/schools/b/media/00000000-0000-0000-0000-000000000001'),
    ).toThrow()
    for (const path of ['//evil.test/api', '/api/../evil', '/api/%2e%2e/evil', '/api/\\evil'])
      await expect(client.requestJson(path)).rejects.toThrow('same-origin')
    expect(transport).not.toHaveBeenCalled()
  })
  it('distinguishes HTTP errors, invalid documents, and cancellation', async () => {
    const transport = vi
        .fn<Transport>()
        .mockResolvedValue(
          response({ errors: [{ code: 'forbidden', detail: '学校授权已撤销' }] }, 403),
        ),
      client = createClient(transport)
    await expect(client.requestResource('/api/v1/schools')).rejects.toMatchObject({
      status: 403,
      code: 'forbidden',
    })
    transport.mockResolvedValue(
      response({
        data: [{ type: 'posts', id: 'p', attributes: { id: 'wrong' } }],
      }),
    )
    await expect(client.requestCollection('/api/v1/schools/s/posts')).rejects.toMatchObject({
      code: 'invalid_document',
    })
    const controller = new AbortController()
    controller.abort()
    await expect(
      client.requestResource('/api/v1/schools', 'GET', undefined, controller.signal),
    ).rejects.toMatchObject({ code: 'cancelled' })
    expect(transport).toHaveBeenCalledTimes(2)
  })
})

import { afterEach, expect, it, vi } from 'vitest'
import { ApiError, requestResource, setCsrf, subscribeUnauthorized } from '../src/http'

afterEach(() => { vi.unstubAllGlobals(); setCsrf('') })

it('notifies expiry for 401 and drops the old CSRF credential', async () => {
  const expired = vi.fn()
  const unsubscribe = subscribeUnauthorized(expired)
  const fetcher = vi.fn().mockResolvedValue(new Response('', { status: 401 }))
  vi.stubGlobal('fetch', fetcher)
  setCsrf('expired-csrf')
  try {
    await expect(requestResource('/api/v1/auth/session')).rejects.toMatchObject({ status: 401 })
    expect(expired).toHaveBeenCalledTimes(1)
    await expect(requestResource('/api/v1/auth/session')).rejects.toBeInstanceOf(ApiError)
    expect(fetcher.mock.calls[0][1].headers['X-CSRF-Token']).toBe('expired-csrf')
    expect(fetcher.mock.calls[1][1].headers['X-CSRF-Token']).toBeUndefined()
    unsubscribe()
    await expect(requestResource('/api/v1/auth/session')).rejects.toBeInstanceOf(ApiError)
    expect(expired).toHaveBeenCalledTimes(2)
  } finally { unsubscribe() }
})

it.each([403, 500])('does not expire browser sessions for HTTP %s', async (status) => {
  const expired = vi.fn()
  const unsubscribe = subscribeUnauthorized(expired)
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status })))
  try {
    await expect(requestResource('/api/v1/auth/session')).rejects.toMatchObject({ status })
    expect(expired).not.toHaveBeenCalled()
  } finally { unsubscribe() }
})

import { afterEach, expect, test, vi } from 'vitest'
import { createDeviceExecutionClient } from '../src/modules/device-execution'
import { mobileDeliveryApi } from '../src/modules/mobile-delivery'
import { ApiError, setCsrf } from '../src/http'
import permit from '../fixtures/mobile-send-permit.json'
import wait from '../fixtures/mobile-execution-wait.json'
afterEach(() => { vi.unstubAllGlobals(); setCsrf('') })
const response = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { 'Content-Type': 'application/vnd.api+json' } })
test('device transport uses fixed HTTPS, no cookie, CSRF, redirect or automatic retry', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response(permit))
  setCsrf('browser-only')
  const client = createDeviceExecutionClient({ baseUrl: 'https://example.test', token: 'dedicated-token', fetch: fetcher })
  const result = await client.permit('claim', { fence: 4, preparation_id: 'prep', snapshot_digest: 'snapshot-sha256' }, 'stable-key')
  expect(result.invocation_id).toBe(permit.data.attributes.invocation_id)
  expect(fetcher).toHaveBeenCalledTimes(1)
  const [url, init] = fetcher.mock.calls[0]
  expect(url).toBe('https://example.test/api/v1/device-execution/claims/claim/permits')
  expect(init).toMatchObject({ credentials: 'omit', redirect: 'error', cache: 'no-store' })
  expect(init?.headers).toMatchObject({ Authorization: 'Bearer dedicated-token', 'Idempotency-Key': 'stable-key' })
  expect(init?.headers).not.toHaveProperty('X-CSRF-Token')
  expect(JSON.parse(String(init?.body))).toEqual({ data: { type: 'mobile-send-permits', attributes: { fence: 4, preparation_id: 'prep', snapshot_digest: 'snapshot-sha256' } } })
  for (const baseUrl of ['http://example.test', 'https://user:pass@example.test', 'https://example.test/path', 'https://example.test/?token=x']) expect(() => createDeviceExecutionClient({ baseUrl })).toThrow()
})
test('normal no-task wait preserves null and its reason', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response(wait))
  const client = createDeviceExecutionClient({ baseUrl: 'https://example.test', fetch: fetcher })
  expect(await client.claim({ binding_revision: 2 }, 'claim-key')).toEqual({ claim: null, wait: wait.meta })
})
test('device errors retain metadata and never retry ambiguous requests', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response({ errors: [{ code: 'unknown_requires_review', detail: 'review', meta: { next_eligible_at: null } }] }, 409))
  const client = createDeviceExecutionClient({ baseUrl: 'https://example.test', fetch: fetcher })
  await expect(client.permit('claim', { fence: 4, preparation_id: 'prep', snapshot_digest: 'digest' }, 'key')).rejects.toMatchObject({ code: 'unknown_requires_review', meta: { next_eligible_at: null } })
  expect(fetcher).toHaveBeenCalledTimes(1)
})
test('management aliases use existing browser CSRF transport and explicit JSON:API types', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response({ data: { type: 'mobile-delivery-previews', id: 'task', attributes: {} } }))
  vi.stubGlobal('fetch', fetcher); setCsrf('csrf')
  await mobileDeliveryApi.preview('school', 'task', { expected_revision: 1 }, 'preview-key')
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data.type).toBe('mobile-delivery-previews')
  expect(fetcher.mock.calls[0][1]?.headers).toMatchObject({ 'X-CSRF-Token': 'csrf' })
  expect(() => mobileDeliveryApi.preview('school', 'task', { expected_revision: 1 }, '')).toThrow()
  expect(new ApiError(409, 'test', 'test', { reason: 'test' }).meta).toEqual({ reason: 'test' })
})

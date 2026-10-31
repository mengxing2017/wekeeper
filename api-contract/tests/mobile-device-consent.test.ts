import { expect, test, vi } from 'vitest'
import { createMobileExecutionDeviceClient } from '../src/modules/mobile-execution/device'
test('device consent uses v6, device bearer and explicit scope without cookies', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-device-consents', id: 'receipt', attributes: { accepted: true, revision: 1 } } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  const input = { grant_id: 'grant', grant_revision: 2, scope_digest: 'a'.repeat(64), expected_revision: 0, accepted: true }
  expect((await api.consent(input, 'stable-key')).accepted).toBe(true)
  expect(transport.mock.calls[0][0]).toBe('https://keeper.example/api/v6/device-execution/consents')
  const request = transport.mock.calls[0][1]
  expect(request).toMatchObject({ credentials: 'omit', redirect: 'error', headers: { Authorization: 'Bearer wk_device_test', 'Idempotency-Key': 'stable-key' } })
  expect(JSON.parse(String(request?.body))).toEqual({ data: { type: 'mobile-device-consents', attributes: input } })
})
test('consent client rejects browser credentials, unsafe origins and missing idempotency', async () => {
  for (const baseUrl of ['http://keeper.example', 'https://keeper.example/api', 'https://user@keeper.example']) {
    expect(() => createMobileExecutionDeviceClient({ baseUrl, token: 'wk_device_test' })).toThrow()
  }
  expect(() => createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'browser-session' })).toThrow()
  const transport = vi.fn<typeof fetch>()
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  await expect(api.consent({ grant_id: 'g', grant_revision: 1, scope_digest: 'a'.repeat(64), expected_revision: 0, accepted: true }, '')).rejects.toThrow()
  expect(transport).not.toHaveBeenCalled()
})
test('context cannot silently accept the wrong resource type', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'old-authorizations', id: 'x', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  await expect(api.authorizationContext()).rejects.toMatchObject({ code: 'invalid_document' })
})
test('shared Rust wire fixture decodes all independent school scopes', async () => {
  const fixture = await import('../fixtures/mobile-device-consent-context.json')
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(fixture.default), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  const context = await api.authorizationContext()
  expect(context.protocol_version).toBe(6)
  expect(context.scopes[0].scope.schema).toBe('business_device_scope/3')
  expect(context.scopes[0].scope.purpose).toBe('通知')
  expect(context.scopes[0].accepted).toBe(false)
  expect(context.scopes[0].consent).toBeNull()
})

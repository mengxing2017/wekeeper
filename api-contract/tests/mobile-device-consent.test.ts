import { expect, test, vi } from 'vitest'
import { createMobileExecutionDeviceClient } from '../src/modules/mobile-execution/device'
test('device consent uses v7, device bearer and explicit scope without cookies', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-device-capability-consents', id: 'receipt', attributes: { accepted: true, revision: 1 } } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  const input = { capability_id: 'capability', configuration_revision: 2, scope_digest: 'a'.repeat(64), expected_revision: 0, accepted: true }
  expect((await api.decideCapability(input, 'stable-key')).accepted).toBe(true)
  expect(transport.mock.calls[0][0]).toBe('https://keeper.example/api/v7/device-execution/capability-consents')
  const request = transport.mock.calls[0][1]
  expect(request).toMatchObject({ credentials: 'omit', redirect: 'error', headers: { Authorization: 'Bearer wk_device_test', 'Idempotency-Key': 'stable-key' } })
  expect(JSON.parse(String(request?.body))).toEqual({ data: { type: 'mobile-device-capability-consents', attributes: input } })
})
test('consent client rejects browser credentials, unsafe origins and missing idempotency', async () => {
  for (const baseUrl of ['http://keeper.example', 'https://keeper.example/api', 'https://user@keeper.example']) {
    expect(() => createMobileExecutionDeviceClient({ baseUrl, token: 'wk_device_test' })).toThrow()
  }
  expect(() => createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'browser-session' })).toThrow()
  const transport = vi.fn<typeof fetch>()
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  await expect(api.decideCapability({ capability_id: 'g', configuration_revision: 1, scope_digest: 'a'.repeat(64), expected_revision: 0, accepted: true }, '')).rejects.toThrow()
  expect(transport).not.toHaveBeenCalled()
})
test('context cannot silently accept the wrong resource type', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'old-authorizations', id: 'x', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  await expect(api.capabilityContext()).rejects.toMatchObject({ code: 'invalid_document' })
})
test('shared Rust wire fixture decodes the global capability scope', async () => {
  const fixture = await import('../fixtures/shared-mobile-capability-context.json')
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(fixture.default), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  const api = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_test', fetch: transport })
  const context = await api.capabilityContext()
  expect(context.protocol_version).toBe(7)
  expect(context.scopes[0].scope.schema).toBe('shared_mobile_execution_scope/1')
  expect(context.scopes[0].scope).not.toHaveProperty('school_id')
  expect(context.scopes[0].scope.credential_generation).toBe(1)
  expect(context.scopes[0].accepted).toBe(false)
  expect(context.scopes[0].consent).toBeNull()
})

test('v7 request failure never probes the v6 school authorization route', async () => {
  const transport=vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({errors:[{code:'not_found'}]}),{status:404,headers:{'Content-Type':'application/vnd.api+json'}}))
  const api=createMobileExecutionDeviceClient({baseUrl:'https://keeper.example',token:'wk_device_test',fetch:transport})
  await expect(api.capabilityContext()).rejects.toMatchObject({status:404})
  expect(transport).toHaveBeenCalledTimes(1)
  expect(transport.mock.calls[0][0]).toBe('https://keeper.example/api/v7/device-execution/capability-context')
  expect(api).not.toHaveProperty('authorizationContext')
  expect(api).not.toHaveProperty('consent')
  expect(api).not.toHaveProperty('validations')
})
test('v7 resource envelope cannot disguise a v6 school scope', async () => {
  const transport=vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({data:{type:'mobile-device-capability-contexts',id:'device',attributes:{protocol_version:6,scopes:[]}}}),{headers:{'Content-Type':'application/vnd.api+json'}}))
  const api=createMobileExecutionDeviceClient({baseUrl:'https://keeper.example',token:'wk_device_test',fetch:transport})
  await expect(api.capabilityContext()).rejects.toMatchObject({code:'invalid_document'})
})

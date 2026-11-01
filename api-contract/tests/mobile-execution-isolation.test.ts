import { afterEach, expect, test, vi } from 'vitest'
import { mobileExecutionApi } from '../src/modules/mobile-execution'
import { setCsrf } from '../src/http'
afterEach(() => { vi.unstubAllGlobals(); setCsrf('') })
test('independent phone targets use JSON:API and preserve exact search text without official identities', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-targets', id: 'target', attributes: { search_text: '备注  群🦦' } } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.createTarget('school', { search_text: '备注  群🦦' }, 'stable-key')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/schools/school/mobile-execution/targets')
  const request = fetcher.mock.calls[0][1]
  expect(JSON.parse(String(request?.body))).toEqual({ data: { type: 'mobile-targets', attributes: { search_text: '备注  群🦦' } } })
  expect(request?.headers).toMatchObject({ 'Idempotency-Key': 'stable-key' })
})
test('shared binding uses platform namespace and exact per-device search text without school grants', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-target-bindings', id: 'binding', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.bind({ target_id: 'target', device_ids: ['phone'], search_text_override: '备注  群🦦' }, 'key')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/platform/mobile-execution/target-bindings')
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual({ data: { type: 'mobile-target-bindings', attributes: { target_id: 'target', device_ids: ['phone'], search_text_override: '备注  群🦦' } } })
})
test('clearing an override and disabling a binding use revisioned platform writes', async () => {
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => new Response(JSON.stringify({ data: { type: 'mobile-target-bindings', id: 'binding', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.updateBinding('binding', { expected_revision: 3, enabled: true, search_text_override: null }, 'edit')
  await mobileExecutionApi.unbind('binding', 4, '维护', 'disable')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/platform/mobile-execution/target-bindings/binding')
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data.attributes).toEqual({ expected_revision: 3, enabled: true, search_text_override: null })
  expect(fetcher.mock.calls[1][0]).toBe('/api/v1/platform/mobile-execution/target-bindings/binding/revocations')
})

test('opening references a product release and environment, never administrator flow', async () => {
  const transport=vi.fn<typeof fetch>().mockImplementation(async () => new Response(JSON.stringify({data:{type:'mobile-capability-openings',id:'opening',attributes:{}}}),{headers:{'Content-Type':'application/vnd.api+json'}}))
  vi.stubGlobal('fetch',transport)
  const input={device_id:'device',expected_revision:0,profile_release_id:'release',environment_snapshot_id:'snapshot'}
  await mobileExecutionApi.openCapability(input,'open')
  await mobileExecutionApi.cancelOpening('opening',1,'cancel')
  expect(transport.mock.calls[0][0]).toBe('/api/v1/platform/mobile-execution/capability-openings')
  expect(JSON.parse(String(transport.mock.calls[0][1]?.body))).toEqual({data:{type:'mobile-capability-openings',attributes:input}})
  expect(transport.mock.calls[1][0]).toBe('/api/v1/platform/mobile-execution/capability-openings/opening/cancellations')
  expect(input).not.toHaveProperty('flow')
})

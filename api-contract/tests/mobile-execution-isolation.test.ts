import { afterEach, expect, test, vi } from 'vitest'
import { mobileExecutionApi } from '../src/modules/mobile-execution'
import { setCsrf } from '../src/http'
afterEach(() => { vi.unstubAllGlobals(); setCsrf('') })
test('independent phone targets use JSON:API and preserve exact search text without official identities', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-targets', id: 'target', attributes: { search_text: '备注  群🦦' } } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.createTarget('school', { search_text: '备注  群🦦', device_ids: ['phone'] }, 'stable-key')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/schools/school/mobile-execution/targets-with-bindings')
  const request = fetcher.mock.calls[0][1]
  expect(JSON.parse(String(request?.body))).toEqual({ data: { type: 'mobile-targets', attributes: { search_text: '备注  群🦦', device_ids: ['phone'] } } })
  expect(request?.headers).toMatchObject({ 'Idempotency-Key': 'stable-key' })
})
test('device grants have their own platform resource namespace', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-device-school-grants', id: 'grant', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.grant({ school_id: 'school', device_id: 'phone', purpose: '通知' }, 'key')
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data.type).toBe('mobile-device-school-grants')
})

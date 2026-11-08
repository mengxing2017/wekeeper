import { afterEach, expect, test, vi } from 'vitest'
import { mobileExecutionApi } from '../src/modules/mobile-execution'
import { setCsrf } from '../src/http'
afterEach(() => { vi.unstubAllGlobals(); setCsrf('') })
test('independent phone targets use JSON:API and preserve exact search text without official identities', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-targets', id: 'target', attributes: { search_text: '备注  群🦦' } } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.createTarget('school', { search_text: '备注  群🦦' }, 'stable-key')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/mobile-execution/workspaces/school/targets')
  const request = fetcher.mock.calls[0][1]
  expect(JSON.parse(String(request?.body))).toEqual({ data: { type: 'mobile-targets', attributes: { search_text: '备注  群🦦' } } })
  expect(request?.headers).toMatchObject({ 'Idempotency-Key': 'stable-key' })
})
test('shared binding selects devices for a target without additional group names', async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: { type: 'mobile-target-bindings', id: 'binding', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.bind({ target_id: 'target', device_ids: ['phone'] }, 'key')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/platform/mobile-execution/target-bindings')
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual({ data: { type: 'mobile-target-bindings', attributes: { target_id: 'target', device_ids: ['phone'] } } })
})
test('enabling and disabling a binding use revisioned platform writes', async () => {
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => new Response(JSON.stringify({ data: { type: 'mobile-target-bindings', id: 'binding', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileExecutionApi.updateBinding('binding', { expected_revision: 3, enabled: true }, 'edit')
  await mobileExecutionApi.unbind('binding', 4, '维护', 'disable')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/platform/mobile-execution/target-bindings/binding')
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data.attributes).toEqual({ expected_revision: 3, enabled: true })
  expect(fetcher.mock.calls[1][0]).toBe('/api/v1/platform/mobile-execution/target-bindings/binding/revocations')
})

test('mobile workspaces and member grants use their own JSON:API resource types', async () => {
  const { mobileWorkspaces } = await import('../src/modules/mobile-execution')
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async path => new Response(JSON.stringify({ data: { type: String(path).endsWith('/members') ? 'mobile-workspace-members' : 'mobile-workspaces', id: 'standalone', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', fetcher)
  await mobileWorkspaces.save('standalone', '独立分发')
  await mobileWorkspaces.grant('standalone', 'operator', 'operator')
  expect(fetcher.mock.calls[0][0]).toBe('/api/v1/mobile-execution/workspaces/standalone')
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual({ data: { type: 'mobile-workspaces', attributes: { name: '独立分发' } } })
  expect(fetcher.mock.calls[1][0]).toBe('/api/v1/mobile-execution/workspaces/standalone/members')
  expect(JSON.parse(String(fetcher.mock.calls[1][1]?.body))).toEqual({ data: { type: 'mobile-workspace-members', attributes: { username: 'operator', role: 'operator' } } })
})

import { afterEach, expect, test, vi } from 'vitest'
import { mobileExecutionApi as api, type FlowV2 } from '../src/modules/mobile-execution'
import { createMobileExecutionDeviceClient } from '../src/modules/mobile-execution/device'
import flowFixture from '../fixtures/mobile-execution-flow-v2.json'
import versionFixture from '../fixtures/mobile-execution-flow-version.json'
afterEach(() => vi.unstubAllGlobals())
const response = (type: string, attributes: object = {}) => new Response(JSON.stringify({ data: { type, id: 'id', attributes } }), { headers: { 'Content-Type': 'application/vnd.api+json' } })
test('independent flow registration decodes the shared Rust wire fixture', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(versionFixture), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', transport)
  const version = await api.createFlow('school', { name: '示例', flow: flowFixture as FlowV2 }, 'register')
  expect(version.flow.schema_version).toBe(2)
  expect(version.flow.selection_policy).toBe('first_group_result')
  expect(transport.mock.calls[0][0]).toBe('/api/v1/schools/school/mobile-execution/flow-versions')
  expect(JSON.parse(String(transport.mock.calls[0][1]?.body))).toEqual({ data: { type: 'mobile-execution-flow-versions', attributes: { name: '示例', flow: flowFixture } } })
})
test('validation, publication and assignment writes use their own resource types and versions', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValueOnce(response('mobile-execution-flow-validations')).mockResolvedValueOnce(response('mobile-execution-flow-releases')).mockResolvedValueOnce(response('mobile-execution-assignments')).mockResolvedValueOnce(response('mobile-execution-assignments'))
  vi.stubGlobal('fetch', transport)
  await api.validateFlow('school', { device_id: 'device', flow_version_id: 'flow', search_text: '验证群 备注' }, 'validate')
  await api.releaseFlow('school', { flow_version_id: 'flow', validation_id: 'validation', name: '发布' }, 'release')
  await api.assign('school', { device_id: 'device', release_id: 'release', expected_revision: 0, enabled: true }, 'assign')
  await api.updateAssignment('school', 'assignment', { device_id: 'device', release_id: 'release', expected_revision: 3, enabled: false }, 'stop')
  expect(transport.mock.calls.map(c => JSON.parse(String(c[1]?.body)).data.type)).toEqual(['mobile-execution-flow-validations', 'mobile-execution-flow-releases', 'mobile-execution-assignments', 'mobile-execution-assignments'])
  expect(transport.mock.calls[3][1]?.method).toBe('PATCH')
  expect(JSON.parse(String(transport.mock.calls[3][1]?.body)).data.attributes.expected_revision).toBe(3)
})
test('device validation inbox and receipts are fixed v6 operations authenticated by device', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValueOnce(new Response(JSON.stringify({ data: [{ type: 'mobile-execution-flow-validations', id: 'job', attributes: { challenge: 'nonce', flow: flowFixture } }] }), { headers: { 'Content-Type': 'application/vnd.api+json' } })).mockResolvedValueOnce(response('mobile-execution-flow-validations', { state: 'failed' }))
  const client = createMobileExecutionDeviceClient({ baseUrl: 'https://keeper.example', token: 'wk_device_validation', fetch: transport })
  const jobs = await client.validations()
  expect(jobs[0].challenge).toBe('nonce')
  await client.validationResult('job', { challenge: 'nonce', flow_digest: 'a'.repeat(64), protocol_version: 6, engine_version: 'otter-ui/2', package_name: 'com.tencent.wework', version_code: 1, evidence: '群分类验证失败，未发送', checks: { application_opened: true, group_container_only: false, first_group_visual_order: false, results_stable: true, chat_opened: false, send_control_unique: false, human_pacing: true, no_send_performed: true } }, 'result')
  expect(transport.mock.calls[0][0]).toBe('https://keeper.example/api/v6/device-execution/flow-validations')
  expect(transport.mock.calls[1][1]).toMatchObject({ credentials: 'omit', headers: { Authorization: 'Bearer wk_device_validation', 'Idempotency-Key': 'result' } })
  expect(JSON.parse(String(transport.mock.calls[1][1]?.body)).data.type).toBe('mobile-execution-flow-validations')
})

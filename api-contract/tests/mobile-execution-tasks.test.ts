import { afterEach, expect, test, vi } from 'vitest'
import { mobileExecutionApi as api } from '../src/modules/mobile-execution'
import fixture from '../fixtures/mobile-execution-task-preview-input.json'
afterEach(() => vi.unstubAllGlobals())
const response = (type: string) => new Response(JSON.stringify({ data: { type, id: 'id', attributes: {} } }), { headers: { 'Content-Type': 'application/vnd.api+json' } })
test('preview preserves target ordering and exact body, confirmation only references its digest', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValueOnce(response('mobile-execution-task-previews')).mockResolvedValueOnce(response('mobile-execution-tasks')).mockResolvedValueOnce(response('mobile-execution-tasks'))
  vi.stubGlobal('fetch', transport)
  const input = fixture
  await api.taskPreview('school-a', input, 'preview')
  await api.confirmTask('school-a', { preview_id: 'preview', preview_digest: 'a'.repeat(64), confirmed: true }, 'confirm')
  await api.cancelTask('school-a', 'task-a', 2, '取消未执行任务', 'cancel')
  expect(transport.mock.calls[0][0]).toBe('/api/v1/schools/school-a/mobile-execution/task-previews')
  expect(JSON.parse(String(transport.mock.calls[0][1]?.body))).toEqual({ data: { type: 'mobile-execution-task-previews', attributes: input } })
  expect(JSON.parse(String(transport.mock.calls[1][1]?.body))).toEqual({ data: { type: 'mobile-execution-tasks', attributes: { preview_id: 'preview', preview_digest: 'a'.repeat(64), confirmed: true } } })
  expect(transport.mock.calls[2][0]).toContain('/tasks/task-a/cancellations')
  expect(new Headers(transport.mock.calls[1][1]?.headers).get('Idempotency-Key')).toBe('confirm')
})
test('task listing is scoped to school, device and page', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: [], links: {} }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', transport)
  await api.tasks('school', 2, 'device/a')
  expect(transport.mock.calls[0][0]).toBe('/api/v1/schools/school/mobile-execution/tasks?page[number]=2&page[size]=30&search=&device_id=device%2Fa')
})
test('task preparation queries only the chosen device configuration', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: [] }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', transport)
  await api.assignments('school', 1, 'device')
  expect(transport.mock.calls[0][0]).toBe('/api/v1/schools/school/mobile-execution/assignments?page[number]=1&page[size]=30&search=&device_id=device')
})

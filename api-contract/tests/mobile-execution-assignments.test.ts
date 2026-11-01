import { afterEach, expect, test, vi } from 'vitest'
import { mobileExecutionApi as api } from '../src/modules/mobile-execution'
afterEach(() => vi.unstubAllGlobals())
test('task preparation queries only the chosen device configuration', async () => {
  const transport = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({ data: [] }), { headers: { 'Content-Type': 'application/vnd.api+json' } }))
  vi.stubGlobal('fetch', transport)
  await api.assignments('school', 1, 'device')
  expect(transport.mock.calls[0][0]).toBe('/api/v1/schools/school/mobile-execution/assignments?page[number]=1&page[size]=30&search=&device_id=device')
})

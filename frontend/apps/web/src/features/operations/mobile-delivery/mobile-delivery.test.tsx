import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ExecutionDevices } from './index'
import { api } from './api'
vi.mock('./api', () => ({
  api: {
    devices: vi.fn(),
    accounts: vi.fn(),
    createDevice: vi.fn(),
    enrollment: vi.fn(),
    capabilities: vi.fn(),
    tasks: vi.fn(),
    groups: vi.fn(),
    task: vi.fn(),
    items: vi.fn(),
    events: vi.fn(),
    preview: vi.fn(),
    confirm: vi.fn(),
  },
}))
class Resize {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', Resize)
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: () => ({
    matches: false,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }),
})
afterEach(() => {
  cleanup()
  vi.resetAllMocks()
  vi.unstubAllGlobals()
})
it.each([undefined, {}])('registers a phone without Web Crypto (%s)', async (crypto) => {
  vi.stubGlobal('crypto', crypto)
  vi.mocked(api.devices).mockResolvedValue({ items: [] })
  vi.mocked(api.accounts).mockResolvedValue({ items: [] })
  vi.mocked(api.createDevice).mockResolvedValue({
    id: 'device', revision: 1, name: '测试手机', enabled: true,
    credential_generation: 0, client_instance_id: null, last_seen_at: null,
    created_at: '2026-10-30T00:00:00Z',
  })
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <ExecutionDevices />
    </QueryClientProvider>,
  )
  fireEvent.click(await screen.findByRole('button', { name: /登记手机/ }))
  fireEvent.change(await screen.findByRole('textbox', { name: '设备名称' }), { target: { value: '测试手机' } })
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await screen.findByText('已保存')
  expect(api.createDevice).toHaveBeenCalledExactlyOnceWith(
    { name: '测试手机' },
    expect.stringMatching(/^[A-Za-z0-9_-]{32}$/),
  )
})
it('shows a grouped short code and explains that device ID entry is unnecessary', async () => {
  vi.mocked(api.devices).mockResolvedValue({ items: [{
    id: 'device', revision: 1, name: '测试手机', enabled: false,
    credential_generation: 0, client_instance_id: null, last_seen_at: null,
    created_at: '2026-10-30T00:00:00Z',
  }] })
  vi.mocked(api.accounts).mockResolvedValue({ items: [] })
  vi.mocked(api.enrollment).mockResolvedValue({ id: 'pairing', device_id: 'device', code: '00123456', expires_at: '2026-10-30T00:10:00Z' })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><ExecutionDevices /></QueryClientProvider>)
  const deviceRow = (await screen.findByText('测试手机')).closest('tr')!
  fireEvent.click(within(deviceRow).getByRole('button', { name: '管理设备' }))
  fireEvent.click(await screen.findByRole('button', { name: '生成配对码' }))
  await screen.findByText('0012 3456')
  expect(screen.getByText(/无需输入设备 ID/)).toBeTruthy()
  expect(api.enrollment).toHaveBeenCalledWith('device', { expected_revision: 1 }, expect.any(String))
  fireEvent.click(screen.getByRole('button', { name: '关闭配对码' }))
  expect(screen.queryByText('0012 3456')).toBeNull()
})

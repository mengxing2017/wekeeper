import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { VisibleStrategies } from './visible'
import { api } from './api'
vi.mock('./api', () => ({
  api: {
    strategies: vi.fn(),
    targets: vi.fn(),
    groups: vi.fn(),
    strategyRuns: vi.fn(),
    runStrategy: vi.fn(),
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
})
it('retries an ambiguous run with its same key and opens its draft without confirming', async () => {
  const strategy = {
    id: 'strategy',
    school_id: 'school',
    name: '机器人测试策略',
    sender_account_id: 'account',
    group_id: 'group',
    device_id: 'device',
    text_content: '测试',
    trigger_mode: 'manual_once' as const,
    enabled: true,
    revision: 2,
    created_by: 'admin',
    created_at: '',
    updated_at: '',
  }
  vi.mocked(api.strategies).mockResolvedValue({ items: [strategy] })
  vi.mocked(api.targets).mockResolvedValue({
    id: 'school',
    accounts: [],
    devices: [],
    accounts_has_more: false,
    devices_has_more: false,
  })
  vi.mocked(api.groups).mockResolvedValue({ items: [] })
  vi.mocked(api.runStrategy).mockRejectedValueOnce(new Error('响应丢失')).mockResolvedValueOnce({
    id: 'run',
    school_id: 'school',
    strategy_id: 'strategy',
    strategy_revision: 2,
    task_id: 'draft',
    device_id: 'device',
    device_binding_revision: 1,
    strategy_snapshot: strategy,
    created_by: 'admin',
    created_at: '',
  })
  const open = vi.fn()
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <VisibleStrategies school="school" openTask={open} />
    </QueryClientProvider>,
  )
  const button = await screen.findByRole('button', { name: '执行一次并预览' })
  fireEvent.click(button)
  await screen.findByText('响应丢失')
  expect(open).not.toHaveBeenCalled()
  fireEvent.click(button)
  await waitFor(() => expect(open).toHaveBeenCalledWith('draft'))
  const calls = vi.mocked(api.runStrategy).mock.calls
  expect(calls).toHaveLength(2)
  expect(calls[0]).toEqual(calls[1])
  expect(calls[0]).toEqual(['school', 'strategy', { expected_revision: 2 }, expect.any(String)])
  expect(api.confirm).not.toHaveBeenCalled()
})

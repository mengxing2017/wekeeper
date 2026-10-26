import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Overview } from './index'
import { getReadiness, getSystemInfo } from './api'

vi.mock('./api', () => ({ getReadiness: vi.fn(), getSystemInfo: vi.fn() }))
Object.defineProperty(window, 'matchMedia', { writable: true, value: vi.fn().mockImplementation(() => ({
  matches: false, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(),
})) })
afterEach(cleanup)

function renderOverview() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}><Overview /></QueryClientProvider>)
}

it('shows unavailable database instead of claiming the workspace is ready', async () => {
  vi.mocked(getSystemInfo).mockResolvedValue({ id:'wekeeper', application: 'wekeeper', version: '0.1.0', stage: 'business' })
  vi.mocked(getReadiness).mockRejectedValue(new Error('unavailable'))
  renderOverview()
  expect(await screen.findByText('数据库尚未就绪')).toBeTruthy()
  expect(screen.queryByText('已就绪')).toBeNull()
})

it('shows an actionable error when the API cannot be reached', async () => {
  vi.mocked(getSystemInfo).mockRejectedValue(new Error('无法连接服务'))
  vi.mocked(getReadiness).mockRejectedValue(new Error('unavailable'))
  renderOverview()
  expect(await screen.findByText('API 暂不可用')).toBeTruthy()
  expect(screen.getByText('无法连接服务')).toBeTruthy()
})

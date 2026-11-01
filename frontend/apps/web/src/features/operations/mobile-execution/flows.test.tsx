import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MobileExecutionFlows } from './flows'
import { api } from './api'
vi.mock('./api', () => ({ api: { assignments: vi.fn(), validateFlow: vi.fn(), createFlow: vi.fn() } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
it('学校旧配置只读，不能继续发布学校设备授权流程', async () => {
  vi.mocked(api.assignments).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileExecutionFlows school="school" admin /></QueryClientProvider>)
  expect(await screen.findByText('执行配置正在迁移到共享手机能力管理')).toBeTruthy()
  expect(screen.queryByRole('button', { name: '创建手机验证' })).toBeNull()
  expect(screen.queryByRole('button', { name: '登记新流程' })).toBeNull()
  expect(api.validateFlow).not.toHaveBeenCalled()
})

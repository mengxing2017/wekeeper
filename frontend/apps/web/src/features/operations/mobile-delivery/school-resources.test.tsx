import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { SchoolExecutionResources } from './school-resources'
import { api } from './api'

vi.mock('./api', () => ({ api: { dispatch: { resources: vi.fn() }, devices: vi.fn(), accounts: vi.fn() } }))
beforeEach(() => {
  vi.resetAllMocks()
  Object.defineProperty(window, 'matchMedia', { configurable: true, value: () => ({
    matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {},
  }) })
  vi.mocked(api.dispatch.resources).mockResolvedValue({ items: [{
    id: 'assignment', device_id: 'phone', device_name: '本校手机', sender_account_id: 'sender',
    account_name: '业务发送身份', company_name: '本校企业', control_state: 'offline',
    last_control_heartbeat_at: null, available: true, block_reason: null,
  }] })
})
afterEach(cleanup)
function show(admin: boolean, accounts: boolean) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}><MemoryRouter>
    <SchoolExecutionResources school="school-a" admin={admin} accounts={accounts} />
  </MemoryRouter></QueryClientProvider>)
}
it('运营只读取当前学校资源，没有全平台维护或配置入口', async () => {
  show(false, false)
  expect(await screen.findByText('本校手机')).toBeTruthy()
  expect(api.dispatch.resources).toHaveBeenCalledWith('school-a', 1)
  expect(api.devices).not.toHaveBeenCalled()
  expect(api.accounts).not.toHaveBeenCalled()
  expect(screen.queryByRole('link', { name: '配置本校执行关系' })).toBeNull()
})
it('学校管理员从发送账号页进入本校执行配置', async () => {
  show(true, true)
  expect(await screen.findByText('本校企业')).toBeTruthy()
  expect(screen.getByRole('link', { name: '配置本校执行关系' }).getAttribute('href')).toBe('/admin?school=school-a&view=mobile-delivery&subview=applications')
})

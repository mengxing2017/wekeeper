import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MobileTargets } from './targets'
import { api } from './api'
vi.mock('./api', () => ({api: {targets: vi.fn(), bindings: vi.fn(), devices: vi.fn(),platformBindings:vi.fn(async()=>({items:[]}))}}))
afterEach(() => { cleanup(); vi.resetAllMocks() })
it('operators can inspect independent targets without account registration or official resource calls', async () => {
  vi.mocked(api.targets).mockResolvedValue({ items: [{ id: 'target', search_text: '手机备注  家长群', remark: '', enabled: true, metadata_revision: 1, locator_revision: 1, archived_at: null }] })
  vi.mocked(api.bindings).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileTargets admin={false} /></QueryClientProvider>)
  expect(await screen.findByText('手机备注 家长群')).toBeTruthy()
  expect(screen.queryByRole('button', { name: /^新\s*增$/ })).toBeNull()
  expect(screen.queryByRole('button', { name: '绑定设备' })).toBeNull()
  expect(api.devices).not.toHaveBeenCalled()
  await waitFor(() => expect(api.targets).toHaveBeenCalledWith( 1, ''))
})

it('system administrators create targets without a device grant or binding form', async () => {
  vi.mocked(api.targets).mockResolvedValue({ items: [] })
  vi.mocked(api.bindings).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileTargets admin /></QueryClientProvider>)
  expect(screen.getByRole('link', {name: '前往默认手机群消息业务方案'}).getAttribute('href')).toBe('/admin?view=mobile-business-schemes')
  expect(screen.queryByRole('button', {name: /^开\s*通$/})).toBeNull()
  fireEvent.click(await screen.findByRole('button', { name: /^新\s*增$/ }))
  expect(await screen.findByLabelText('群名称')).toBeTruthy()
  expect(screen.queryByLabelText('企业微信完整群名')).toBeNull()
  expect(screen.queryByLabelText('搜索词')).toBeNull()
  expect(screen.queryByLabelText('平台别名（不用于搜索）')).toBeNull()
  expect(screen.queryByText('同时绑定授权设备')).toBeNull()
  expect(screen.queryByRole('button', { name: '绑定设备' })).toBeNull()
  expect(api.devices).not.toHaveBeenCalled()
})

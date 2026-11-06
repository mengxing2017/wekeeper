import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MobileTargets } from './targets'
import { api, sharedSend } from './api'
vi.mock('./api', () => ({sharedSend:{overview:vi.fn().mockResolvedValue({id:'school',tasks:[],drafts:[],readiness:[],pool:[],inspections:[]})}, api: { targets: vi.fn(), bindings: vi.fn(), devices: vi.fn(), fieldInvitations: vi.fn().mockResolvedValue({items:[]}), schoolFieldChecks: vi.fn().mockResolvedValue({items:[]}) } }))
beforeEach(()=>{vi.mocked(sharedSend.overview).mockResolvedValue({id:'school',tasks:[],drafts:[],readiness:[],pool:[],inspections:[]})})
afterEach(() => { cleanup(); vi.resetAllMocks() })
it('operators can inspect independent targets without account registration or official resource calls', async () => {
  vi.mocked(api.targets).mockResolvedValue({ items: [{ id: 'target', school_id: 'school', search_text: '手机备注  家长群', alias: '通知群', remark: '', enabled: true, metadata_revision: 1, locator_revision: 1, archived_at: null }] })
  vi.mocked(api.bindings).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileTargets school="school" admin={false} /></QueryClientProvider>)
  expect(await screen.findByText('手机备注 家长群')).toBeTruthy()
  expect(screen.queryByRole('button', { name: '新增目标群' })).toBeNull()
  expect(screen.queryByRole('button', { name: '绑定设备' })).toBeNull()
  expect(api.devices).not.toHaveBeenCalled()
  await waitFor(() => expect(api.targets).toHaveBeenCalledWith('school', 1, ''))
})

it('school admins create targets without a device grant or binding form', async () => {
  vi.mocked(api.targets).mockResolvedValue({ items: [] })
  vi.mocked(api.bindings).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileTargets school="school" admin /></QueryClientProvider>)
  fireEvent.click(await screen.findByRole('button', { name: '新增目标群' }))
  expect(await screen.findByLabelText('企业微信完整群名')).toBeTruthy()
  expect(screen.queryByText('同时绑定本校授权设备')).toBeNull()
  expect(screen.queryByRole('button', { name: '绑定设备' })).toBeNull()
  expect(api.devices).not.toHaveBeenCalled()
})

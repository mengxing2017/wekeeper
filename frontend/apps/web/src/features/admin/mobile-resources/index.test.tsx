import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { SharedMobileResources } from './index'
import { api } from './api'
vi.mock('./api', () => ({ api: { platformBindings: vi.fn(), platformDevices: vi.fn(), platformTargets: vi.fn(), updateBinding: vi.fn(), bind: vi.fn(), unbind: vi.fn() } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
it('平台从具体目标群绑定共享手机，不再填写学校设备用途授权', async () => {
  vi.mocked(api.platformBindings).mockResolvedValue({ items: [] })
  vi.mocked(api.platformDevices).mockResolvedValue({ items: [] })
  vi.mocked(api.platformTargets).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><SharedMobileResources /></QueryClientProvider>)
  fireEvent.click(await screen.findByRole('button', { name: '绑定共享手机' }))
  expect(await screen.findByText('为目标群批量绑定手机')).toBeTruthy()
  expect(screen.getByLabelText('学校目标群')).toBeTruthy()
  expect(screen.getByLabelText('该手机下发群名（留空继承群默认名）')).toBeTruthy()
  expect(screen.queryByLabelText('明确授权用途')).toBeNull()
  expect(api.platformTargets).toHaveBeenCalledWith(1, '')
})

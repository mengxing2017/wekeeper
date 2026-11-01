import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { DeviceCapabilities } from './capabilities'
import { api } from './api'
vi.mock('./api', () => ({ api: { platformCapabilities: vi.fn(), platformDevices: vi.fn(), createCapability: vi.fn(), updateCapability: vi.fn() } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
it('配置启用不冒充手机同意，新增配置默认停用且必须导入', async () => {
  vi.mocked(api.platformCapabilities).mockResolvedValue({ items: [] })
  vi.mocked(api.platformDevices).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><DeviceCapabilities /></QueryClientProvider>)
  expect(await screen.findByText('配置启用不代表设备可以发送')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '新增设备配置' }))
  expect(await screen.findByLabelText('导入 v7 设备能力配置')).toBeTruthy()
  expect(screen.getByRole('switch').getAttribute('aria-checked')).toBe('false')
  fireEvent.change(screen.getByLabelText('导入 v7 设备能力配置'), { target: { files: [{ size: 100, text: async () => JSON.stringify({ protocol_version: 6 }) }] } })
  expect(await screen.findByText('请选择 v7 企业微信群文字配置文件；旧学校配置不能直接导入')).toBeTruthy()
  expect(api.createCapability).not.toHaveBeenCalled()
})

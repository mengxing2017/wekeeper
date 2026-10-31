import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { DeviceStateEditor } from './device-state'
import { api, type Device } from './api'
import { selectOption } from '../../../test/select-option'

vi.mock('./api', () => ({ api: { device: vi.fn(), updateDevice: vi.fn() } }))
const device: Device = {
  id: 'phone', name: '测试手机', enabled: true, revision: 1, credential_generation: 1,
  client_instance_id: null, last_seen_at: null, created_at: '2026-10-31T00:00:00Z',
}
beforeEach(() => {
  vi.resetAllMocks()
  Object.defineProperty(window, 'matchMedia', { configurable: true, value: () => ({
    matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {},
  }) })
  vi.mocked(api.device).mockResolvedValue(device)
  vi.mocked(api.updateDevice).mockResolvedValue({ ...device, enabled: false, revision: 2 })
})
afterEach(cleanup)

async function requestDisable() {
  await selectOption(screen.getByLabelText('设备状态'), '停用')
  fireEvent.change(screen.getByRole('textbox', { name: '变更原因' }), { target: { value: '管理员暂停本机执行' } })
  fireEvent.click(screen.getByRole('button', { name: '保存设备状态' }))
}

it('reads the current device and saves an explicit state change with the matching revision', async () => {
  const refresh = vi.fn().mockResolvedValue(undefined)
  render(<DeviceStateEditor device={device} refresh={refresh} />)
  await requestDisable()
  expect(await screen.findByText('设备状态已保存')).toBeTruthy()
  expect(api.updateDevice).toHaveBeenCalledExactlyOnceWith('phone', {
    expected_revision: 1, enabled: false, reason: '管理员暂停本机执行',
  }, expect.any(String))
  expect(api.device).toHaveBeenCalledTimes(1)
  expect(refresh).toHaveBeenCalledTimes(1)
})

it('keeps the draft when pairing advances the revision and requires confirmation before any PATCH', async () => {
  const latest = { ...device, revision: 2, credential_generation: 2 }
  vi.mocked(api.device).mockResolvedValue(latest)
  vi.mocked(api.updateDevice).mockResolvedValue({ ...latest, enabled: false, revision: 3 })
  const refresh = vi.fn().mockResolvedValue(undefined)
  const view = render(<DeviceStateEditor device={device} refresh={refresh} />)
  await requestDisable()
  expect(await screen.findByText(/设备信息已更新，已读取最新状态/)).toBeTruthy()
  expect(api.updateDevice).not.toHaveBeenCalled()
  view.rerender(<DeviceStateEditor device={latest} refresh={refresh} />)
  expect((screen.getByRole('textbox', { name: '变更原因' }) as HTMLInputElement).value).toBe('管理员暂停本机执行')
  fireEvent.click(screen.getByRole('button', { name: '确认并保存设备状态' }))
  expect(await screen.findByText('设备状态已保存')).toBeTruthy()
  expect(api.updateDevice).toHaveBeenCalledExactlyOnceWith('phone', {
    expected_revision: 2, enabled: false, reason: '管理员暂停本机执行',
  }, expect.any(String))
})

it('handles a concurrent PATCH conflict without replaying or losing the entered reason', async () => {
  vi.mocked(api.device).mockResolvedValueOnce(device).mockResolvedValue({ ...device, revision: 2 })
  vi.mocked(api.updateDevice).mockRejectedValueOnce(Object.assign(new Error('手机执行约束未满足'), { code: 'revision_conflict' }))
  render(<DeviceStateEditor device={device} refresh={vi.fn().mockResolvedValue(undefined)} />)
  await requestDisable()
  expect(await screen.findByText(/设备信息已更新，已读取最新状态/)).toBeTruthy()
  expect(api.updateDevice).toHaveBeenCalledTimes(1)
  expect(screen.queryByText('手机执行约束未满足')).toBeNull()
  expect((screen.getByRole('textbox', { name: '变更原因' }) as HTMLInputElement).value).toBe('管理员暂停本机执行')
  fireEvent.click(screen.getByRole('button', { name: '确认并保存设备状态' }))
  await screen.findByText('设备状态已保存')
  expect(api.updateDevice).toHaveBeenCalledTimes(2)
  expect(vi.mocked(api.updateDevice).mock.calls[1][1].expected_revision).toBe(2)
})

it('blocks writes while current state cannot be read and preserves input for an explicit refresh', async () => {
  vi.mocked(api.device).mockRejectedValueOnce(new Error('网络中断')).mockResolvedValue({ ...device, revision: 3 })
  render(<DeviceStateEditor device={device} refresh={vi.fn().mockResolvedValue(undefined)} />)
  await requestDisable()
  expect(await screen.findByText('网络中断')).toBeTruthy()
  expect(api.updateDevice).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: '刷新设备状态' }))
  await waitFor(() => expect(screen.getByRole('button', { name: '确认并保存设备状态' })).toBeTruthy())
  expect((screen.getByRole('textbox', { name: '变更原因' }) as HTMLInputElement).value).toBe('管理员暂停本机执行')
  expect(api.updateDevice).not.toHaveBeenCalled()
})

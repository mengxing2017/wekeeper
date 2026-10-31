import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { DeviceAccountBinding } from './device-account-binding'
import { api, type Device, type SendingAccount } from './api'
vi.mock('./api', () => ({ api: { bindDevice: vi.fn() } }))
vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} })
Object.defineProperty(window, 'matchMedia', { writable: true, value: () => ({ matches: false, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn() }) })
afterEach(() => { cleanup(); vi.resetAllMocks() })
const device: Device = { id: 'phone', name: '手机', revision: 2, enabled: true, credential_generation: 1, client_instance_id: null, last_seen_at: null, created_at: '' }
const account: SendingAccount = { id: 'sender', revision: 1, client_kind: 'wecom', identity_namespace: null, external_key: null, visible_profile: { company_name: '学校', account_name: '张老师' }, responsible_actor_id: 'admin', identity_evidence_ref: '登记说明', verified_at: '', enabled: true, daily_limit: 30, device_binding_eligibility: { eligible: true, reason: null } }
async function select() {
  fireEvent.mouseDown(screen.getByRole('combobox', { name: '发送账号' }))
  fireEvent.click(await screen.findByText('学校 · 张老师'))
}
it('requires explicit phone identity confirmation but permits an empty note', async () => {
  const refresh = vi.fn().mockResolvedValue(undefined)
  vi.mocked(api.bindDevice).mockResolvedValue({ id: 'phone', device_id: 'phone', sender_account_id: 'sender', revision: 3, identity_evidence_ref: '' })
  render(<DeviceAccountBinding device={device} accounts={[account]} refresh={refresh} />)
  await select()
  fireEvent.click(screen.getByRole('button', { name: '绑定发送账号' }))
  await screen.findByText('请先在这台手机核对当前账号')
  expect(api.bindDevice).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('checkbox'))
  fireEvent.click(screen.getByRole('button', { name: '绑定发送账号' }))
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce())
  expect(api.bindDevice).toHaveBeenCalledWith('phone', { expected_revision: 2, sender_account_id: 'sender', identity_evidence_ref: '' }, expect.any(String))
})
it('explains unavailable school authorization before submission', () => {
  render(<DeviceAccountBinding device={device} accounts={[{ ...account, device_binding_eligibility: { eligible: false, reason: 'school_authorization_required' } }]} refresh={vi.fn()} />)
  expect(screen.getByText(/请先配置目标群与账号授权/)).toBeTruthy()
  expect(screen.getByRole('button', { name: '绑定发送账号' }).hasAttribute('disabled')).toBe(true)
})
it('preserves a business error and uses the same idempotency key when retrying the same binding', async () => {
  vi.mocked(api.bindDevice).mockRejectedValue(new Error('该发送账号缺少有效学校群授权'))
  render(<DeviceAccountBinding device={device} accounts={[account]} refresh={vi.fn()} />)
  await select()
  fireEvent.click(screen.getByRole('checkbox'))
  fireEvent.click(screen.getByRole('button', { name: '绑定发送账号' }))
  await screen.findByText('该发送账号缺少有效学校群授权')
  fireEvent.click(screen.getByRole('button', { name: '绑定发送账号' }))
  await waitFor(() => expect(api.bindDevice).toHaveBeenCalledTimes(2))
  expect(vi.mocked(api.bindDevice).mock.calls[0]).toEqual(vi.mocked(api.bindDevice).mock.calls[1])
})

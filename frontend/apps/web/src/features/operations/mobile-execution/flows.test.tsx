import { Modal } from 'antd'
import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MobileExecutionFlows } from './flows'
import { api } from './api'
vi.mock('./api', () => ({ api: { flows: vi.fn(), validations: vi.fn(), releases: vi.fn(), assignments: vi.fn(), devices: vi.fn(), releaseFlow: vi.fn() } }))
afterEach(() => { Modal.destroyAll(); cleanup(); vi.resetAllMocks() })
function setup(admin: boolean) {
  vi.mocked(api.flows).mockResolvedValue({ items: [] })
  vi.mocked(api.validations).mockResolvedValue({ items: [{ id: 'validation', school_id: 'school', device_id: 'phone', flow_version_id: 'flow', grant_id: 'grant', grant_revision: 1, consent_id: 'consent', consent_revision: 1, search_text: '测试群', state: 'passed', expires_at: '2026-11-02', completed_at: '2026-11-01', flow_name: '现场流程', device_name: '教务手机', result: null }] })
  vi.mocked(api.releases).mockResolvedValue({ items: [] })
  vi.mocked(api.assignments).mockResolvedValue({ items: [{ id: 'assignment', school_id: 'school', device_id: 'phone', grant_id: 'grant', release_id: 'release', enabled: true, execution_revision: 1, metadata_revision: 1, device_name: '教学设备', release_name: '已发布流程', consent_required: true, available: false }] })
  vi.mocked(api.devices).mockResolvedValue({ items: [] })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileExecutionFlows school="school" admin={admin} /></QueryClientProvider>)
}
it('operators see exact preparation blockers and cannot publish or impersonate device validation', async () => {
  setup(false)
  expect(await screen.findByText('请在手机接受当前学校授权')).toBeTruthy()
  expect(screen.queryByRole('button', { name: '登记新流程' })).toBeNull()
  expect(screen.queryByRole('button', { name: '发布流程' })).toBeNull()
  expect(screen.queryByRole('button', { name: '配置设备流程' })).toBeNull()
  expect(api.devices).not.toHaveBeenCalled()
})
it('administrator publishes a selected device validation with explicit confirmation', async () => {
  vi.mocked(api.releaseFlow).mockResolvedValue({ id: 'release', school_id: 'school', flow_version_id: 'flow', validation_id: 'validation', name: '现场流程（设备验证发布）', enabled: true, revision: 1 })
  setup(true)
  fireEvent.click(await screen.findByRole('button', { name: '发布流程' }))
  expect(api.releaseFlow).not.toHaveBeenCalled()
  expect((await screen.findAllByText('发布此设备已验证的流程？')).length).toBeGreaterThan(0)
  fireEvent.click(screen.getByRole('button', { name: /OK|确.*定/u }))
  await waitFor(() => expect(api.releaseFlow).toHaveBeenCalledWith('school', { flow_version_id: 'flow', validation_id: 'validation', name: '现场流程（设备验证发布）' }, expect.any(String)))
})

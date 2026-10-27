import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import MobileDelivery from './index'
import { api } from './api'
vi.mock('./api', () => ({ api: { capabilities: vi.fn(), tasks: vi.fn(), groups: vi.fn(), task: vi.fn(), items: vi.fn(), events: vi.fn(), preview: vi.fn(), confirm: vi.fn() } }))
class Resize { observe() {} unobserve() {} disconnect() {} }
vi.stubGlobal('ResizeObserver', Resize)
Object.defineProperty(window, 'matchMedia', { writable: true, value: () => ({ matches: false, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn() }) })
afterEach(() => { cleanup(); vi.resetAllMocks() })
function show(role?: string) { render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileDelivery school="school" role={role} /></QueryClientProvider>) }
it('requires actual school authorization before requesting business data', () => {
  show()
  expect(screen.getByText(/平台管理员身份不代替学校授权/)).toBeTruthy()
  expect(api.capabilities).not.toHaveBeenCalled()
  expect(api.tasks).not.toHaveBeenCalled()
})
it('shows disabled execution and requires all manual reviews before confirming the frozen digest', async () => {
  const task = { id: 'task-123456', revision: 2, school_id: 'school', creator_id: 'admin', confirmed_by: null, activity_id: '校园活动', state: 'draft' as const, review_evidence_ref: null, created_at: '2026-10-26T00:00:00Z' }
  vi.mocked(api.capabilities).mockResolvedValue({ id: 'school', create: false, confirm: true, claim_enabled: false, reason: 'execution_disabled', quota_summary: [], quota_summary_limit: 100, settings: { id: 'school', school_id: 'school', revision: 0, enabled: false, history_reviewed: false }, minimum_protocol_version: 1, account_default_limit: 30, account_max_limit: 50, group_default_limit: 10, group_max_limit: 30 })
  vi.mocked(api.tasks).mockResolvedValue({ items: [task] })
  vi.mocked(api.groups).mockResolvedValue({ items: [] })
  vi.mocked(api.task).mockResolvedValue(task)
  vi.mocked(api.items).mockResolvedValue({ items: [] })
  vi.mocked(api.events).mockResolvedValue({ items: [] })
  vi.mocked(api.preview).mockResolvedValue({ id: task.id, expected_revision: 2, preview_digest: 'frozen-digest', requires_substantive_difference_review: true, items: [{ id: 'item', group_id: 'group', content: '本群专属通知', snapshot_digest: 'snapshot', reason: null, next_eligible_at: null }] })
  vi.mocked(api.confirm).mockResolvedValue({ ...task, state: 'confirmed' })
  show('admin')
  await screen.findByText('execution_disabled', {}, { timeout: 5000 })
  expect(screen.queryByRole('columnheader', { name: '每日限额' })).toBeNull()
  fireEvent.click(screen.getByRole('tab', { name: '目标群' }))
  expect(await screen.findByRole('columnheader', { name: '每日限额' })).toBeTruthy()
  expect(screen.queryByRole('columnheader', { name: '活动标识' })).toBeNull()
  fireEvent.click(screen.getByRole('tab', { name: '发送任务' }))
  fireEvent.click(await screen.findByRole('button', { name: '查看手机任务 123456' }, { timeout: 5000 }))
  fireEvent.click(await screen.findByRole('button', { name: '生成冻结内容预览' }, { timeout: 5000 }))
  await screen.findByText('本群专属通知', {}, { timeout: 5000 })
  const confirm = screen.getByRole('button', { name: '人工确认并进入执行队列' })
  expect(confirm.hasAttribute('disabled')).toBe(true)
  expect(api.confirm).not.toHaveBeenCalled()
  fireEvent.click(screen.getByLabelText('已人工审核各群内容'))
  fireEvent.click(screen.getByLabelText('已核对目标群、授权和发送范围'))
  fireEvent.change(screen.getByLabelText('人工确认依据'), { target: { value: 'review-001' } })
  expect(confirm.hasAttribute('disabled')).toBe(true)
  fireEvent.click(screen.getByLabelText('已核对同校不同群内容的实质差异'))
  fireEvent.click(confirm)
  await waitFor(() => expect(api.confirm).toHaveBeenCalledWith('school', task.id, { expected_revision: 2, preview_digest: 'frozen-digest', content_review: true, scope_acceptance: true, substantive_difference_reviewed: true, evidence_ref: 'review-001' }, expect.any(String)))
})

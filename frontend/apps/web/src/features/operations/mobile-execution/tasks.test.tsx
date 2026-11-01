import { afterAll, afterEach, expect, it, vi } from 'vitest'
import { cleanup, configure, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MobileExecutionTasks } from './tasks'
import { api, type MobileTarget, type TaskPlanSnapshot, type ExecutionTaskDetail } from './api'
vi.mock('./api', () => ({ api: { devices: vi.fn(), assignments: vi.fn(), targets: vi.fn(), tasks: vi.fn(), taskPreview: vi.fn(), confirmTask: vi.fn(), cancelTask: vi.fn() } }))
configure({ asyncUtilTimeout: 5000 })
afterAll(() => configure({ asyncUtilTimeout: 1000 }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
const target = (id: string): MobileTarget => ({ id, school_id: 'school', search_text: `原始  ${id}`, alias: id, remark: '', enabled: true, metadata_revision: 1, locator_revision: 1, archived_at: null })
const snapshot: TaskPlanSnapshot = { protocol_version: 6, school_id: 'school', selection_policy: 'first_group_result', message: ' 原文\n正文 ', configuration: { device_id: 'phone', credential_generation: 1, grant_id: 'grant', grant_revision: 1, consent_id: 'consent', consent_revision: 1, assignment_id: 'assignment', assignment_revision: 1, release_id: 'release', release_revision: 1, flow_version_id: 'flow', flow_digest: 'a'.repeat(64), flow: { application_label: '企业微信', pacing: { between_targets_min_ms: 1000 } } as TaskPlanSnapshot['configuration']['flow'] }, items: [{ target_id: 'second', search_text: '原始  second', locator_revision: 1, binding_id: 'binding', binding_revision: 1, position: 1 }, { target_id: 'first', search_text: '原始  first', locator_revision: 1, binding_id: 'binding2', binding_revision: 1, position: 2 }] }
function setup(admin = true, initialTasks: ExecutionTaskDetail[] = []) {
  vi.mocked(api.devices).mockResolvedValue({ items: [{ id: 'phone', name: '教务手机', school_alias: '', school_remark: '', enabled: true, control_state: 'online', grant_id: 'grant', grant_revision: 1, last_seen_at: null, last_control_heartbeat_at: null }] })
  vi.mocked(api.assignments).mockResolvedValue({ items: [{ id: 'assignment', school_id: 'school', device_id: 'phone', grant_id: 'grant', release_id: 'release', enabled: true, execution_revision: 1, metadata_revision: 1, available: true }] })
  vi.mocked(api.targets).mockImplementation(async (_school, page) => ({ items: [target(page === 2 ? 'second' : 'first')], page: { number: page ?? 1, size: 30, has_more: page !== 2 } }))
  vi.mocked(api.tasks).mockResolvedValue({ items: initialTasks })
  vi.mocked(api.taskPreview).mockResolvedValue({ id: 'preview', school_id: 'school', device_id: 'phone', input: { device_id: 'phone', target_ids: ['second', 'first'], message: snapshot.message }, snapshot, digest: 'b'.repeat(64), created_by: 'actor', created_at: '2026-11-01', expires_at: '2099-11-01T00:00:00Z' })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const view = (school: string, isAdmin: boolean) => <QueryClientProvider client={client}><MobileExecutionTasks school={school} admin={isAdmin} /></QueryClientProvider>
  const result = render(view('school', admin))
  return { ...result, switchScope: (school: string, isAdmin: boolean) => result.rerender(view(school, isAdmin)) }
}
async function chooseAndPreview() {
  fireEvent.click(await screen.findByRole('button', { name: '使用教务手机' }))
  fireEvent.click(await screen.findByRole('checkbox', { name: '选择目标first' }))
  fireEvent.click(screen.getByRole('button', { name: '下一页任务目标' }))
  fireEvent.click(await screen.findByRole('checkbox', { name: '选择目标second' }))
  fireEvent.click(screen.getByRole('button', { name: '上移目标2' }))
  fireEvent.change(screen.getByRole('textbox', { name: '手机任务消息正文' }), { target: { value: snapshot.message } })
  fireEvent.click(screen.getByRole('button', { name: '预览全部目标和正文' }))
  return screen.findByRole('dialog', { name: '4. 核对全部目标并确认任务' })
}
it('retains ordered cross-page selection, previews exact body and requires explicit review', async () => {
  setup()
  const dialog = await chooseAndPreview()
  expect(api.assignments).toHaveBeenCalledWith('school', 1, 'phone')
  expect(api.taskPreview).toHaveBeenCalledWith('school', { device_id: 'phone', target_ids: ['second', 'first'], message: snapshot.message }, expect.any(String))
  expect(dialog.textContent).toContain('群间最短间隔 1 秒')
  expect(within(dialog).getByRole('button', { name: '确认任务' }).hasAttribute('disabled')).toBe(true)
  expect(api.confirmTask).not.toHaveBeenCalled()
  vi.mocked(api.confirmTask).mockRejectedValueOnce(new Error('网络中断')).mockResolvedValueOnce({ id: 'task' } as ExecutionTaskDetail)
  fireEvent.click(within(dialog).getByRole('checkbox'))
  fireEvent.click(within(dialog).getByRole('button', { name: '确认任务' }))
  fireEvent.click(await within(dialog).findByRole('button', { name: '重试同一次确认' }))
  await waitFor(() => expect(api.confirmTask).toHaveBeenCalledTimes(2))
  expect(vi.mocked(api.confirmTask).mock.calls[0]).toEqual(vi.mocked(api.confirmTask).mock.calls[1])
  expect(await screen.findByText(/任务已登记并预留手机/)).toBeTruthy()
}, 40000)
it('operator can preview but cannot confirm, and a school/role change discards old dialogs and drafts', async () => {
  const result = setup(false)
  const dialog = await chooseAndPreview()
  expect(within(dialog).queryByRole('button', { name: '确认任务' })).toBeNull()
  expect(within(dialog).getByText('请由本校管理员重新预览并人工确认。')).toBeTruthy()
  result.switchScope('school-b', true)
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect((screen.getByRole('textbox', { name: '手机任务消息正文' }) as HTMLTextAreaElement).value).toBe('')
  expect(api.confirmTask).not.toHaveBeenCalled()
}, 40000)
it('select current page never selects unseen targets and configuration blocker prevents preview', async () => {
  setup()
  vi.mocked(api.assignments).mockResolvedValue({ items: [] })
  fireEvent.click(await screen.findByRole('button', { name: '使用教务手机' }))
  expect(await screen.findByText(/尚未配置执行流程/)).toBeTruthy()
  await screen.findByRole('checkbox', { name: '选择目标first' })
  fireEvent.click(screen.getByRole('button', { name: '选择本页（最多 20 个）' }))
  const list = screen.getByRole('list', { name: '已选目标执行顺序' })
  expect(within(list).getAllByRole('listitem')).toHaveLength(1)
  expect(list.textContent).toContain('first')
  expect(list.textContent).not.toContain('second')
  expect(screen.getByRole('button', { name: '预览全部目标和正文' }).hasAttribute('disabled')).toBe(true)
})
it('shows original plan and only allows cancellation of fully unclaimed queued work', async () => {
  const task = { id: 'task', device_name: '教务手机', revision: 1, state: 'queued', plan: snapshot, items: snapshot.items.map((s, i) => ({ id: `item${i}`, position: i + 1, snapshot: s, state: 'pending' })) } as ExecutionTaskDetail
  setup(true, [task, { ...task, id: 'unknown-task', items: [{ ...task.items[0], state: 'unknown' }] }])
  const cancel = await screen.findByRole('button', { name: '取消未领取任务' })
  expect(screen.getAllByRole('button', { name: '取消未领取任务' })).toHaveLength(1)
  fireEvent.click(cancel)
  expect(api.cancelTask).not.toHaveBeenCalled()
  vi.mocked(api.cancelTask).mockResolvedValue({ ...task, state: 'cancelled' })
  fireEvent.click(await screen.findByRole('button', { name: '确认取消任务' }))
  await waitFor(() => expect(api.cancelTask).toHaveBeenCalledWith('school', 'task', 1, '使用者取消尚未领取的手机任务', 'mobile-task-cancel:task:1'))
})

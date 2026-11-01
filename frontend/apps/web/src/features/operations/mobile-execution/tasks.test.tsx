import { afterAll, afterEach, expect, it, vi } from 'vitest'
import { cleanup, configure, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MobileExecutionTasks } from './tasks'
import { api, sharedSend, type TaskPlanSnapshot, type ExecutionTaskDetail } from './api'
vi.mock('./api', () => ({ sharedSend:{overview:vi.fn()},api: { devices: vi.fn(), assignments: vi.fn(), targets: vi.fn(), tasks: vi.fn(), taskPreview: vi.fn(), confirmTask: vi.fn(), cancelTask: vi.fn() } }))
configure({ asyncUtilTimeout: 5000 })
afterAll(() => configure({ asyncUtilTimeout: 1000 }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
const snapshot: TaskPlanSnapshot = { protocol_version: 6, school_id: 'school', selection_policy: 'first_group_result', message: ' 原文\n正文 ', configuration: { device_id: 'phone', credential_generation: 1, grant_id: 'grant', grant_revision: 1, consent_id: 'consent', consent_revision: 1, assignment_id: 'assignment', assignment_revision: 1, release_id: 'release', release_revision: 1, flow_version_id: 'flow', flow_digest: 'a'.repeat(64), flow: { application_label: '企业微信', pacing: { between_targets_min_ms: 1000 } } as TaskPlanSnapshot['configuration']['flow'] }, items: [{ target_id: 'second', search_text: '原始  second', locator_revision: 1, binding_id: 'binding', binding_revision: 1, position: 1 }, { target_id: 'first', search_text: '原始  first', locator_revision: 1, binding_id: 'binding2', binding_revision: 1, position: 2 }] }
function setup(admin = true, initialTasks: ExecutionTaskDetail[] = []) {
  vi.mocked(sharedSend.overview).mockResolvedValue({id:"school",tasks:[],drafts:[],inspections:[]})
  vi.mocked(api.targets).mockResolvedValue({items:[]})
  vi.mocked(api.tasks).mockResolvedValue({ items: initialTasks })
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MobileExecutionTasks school="school" admin={admin} /></QueryClientProvider>)
}
it('共享升级关闭旧创建入口，保留学校历史记录', async () => {
  setup()
  expect(await screen.findByText('历史手机任务仅供查询与核对')).toBeTruthy()
  expect(screen.queryByText('预览全部目标')).toBeNull()
  expect(screen.queryByText('选择手机')).toBeNull()
  expect(api.devices).not.toHaveBeenCalled()
  expect(api.taskPreview).not.toHaveBeenCalled()
})
it('unknown 保留原搜索文字与结果，不能普通取消', async () => {
  const item = { id: 'item', school_id: 'school', task_id: 'task', intent_id: 'intent', device_id: 'phone', target_id: 'second', binding_id: 'binding', position: 1, attempt: 1, snapshot: snapshot.items[0], state: 'unknown' as const }
  const task: ExecutionTaskDetail = { id: 'task', school_id: 'school', intent_id: 'intent', device_id: 'phone', preview_id: 'preview', state: 'queued', revision: 1, created_by: 'actor', confirmed_by: 'actor', created_at: '2026-11-01T00:00:00Z', cancelled_at: null, device_name: '旧手机', plan: snapshot, plan_digest: 'a'.repeat(64), items: [item] }
  setup(true, [task])
  fireEvent.click(await screen.findByText('查看原计划'))
  expect(await screen.findByText('结果不确定，待核对')).toBeTruthy()
  expect(screen.getByText('原始 second').textContent).toBe('原始  second')
  expect(screen.queryByRole('button', { name: '取消未领取任务' })).toBeNull()
})

import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MobileDispatchRecords } from './dispatch-records'
import { dispatchRecords, type DispatchIntake } from './api'
vi.mock('./api', () => ({
  dispatchRecords: {
    list: vi.fn(),
    detail: vi.fn(),
    operations: vi.fn(),
    checks: vi.fn(),
    events: vi.fn(),
    exchanges: vi.fn(),
    exchangeChecks: vi.fn(),
  },
}))
const empty = {
  items: [],
  has_more: false,
  next_cursor: null,
  version: 'v1',
  observed_at: '2026-11-16T06:00:00Z',
}
const intake: DispatchIntake = {
  id: 'intake-1',
  source_kind: 'source_route',
  message_id: 'message',
  receipt_id: 'receipt',
  configuration_id: 'config',
  target_id: 'target',
  target_name: '冻结群',
  task_id: null,
  item_id: null,
  created_at: empty.observed_at,
  creation_state: 'rejected',
  content_summary: '消息摘要',
  device_id: null,
  device_name: null,
  attempt_id: null,
  attempt_no: null,
  execution_id: null,
  generated_at: null,
  received_at: null,
  result_deadline_at: null,
  round_deadline_at: null,
  completed_rounds: null,
  max_rounds: null,
  next_evaluation_at: null,
  outcome: 'failed',
  stage: 'rejected',
  reason_code: 'mobile_candidates_missing',
  reason_label: '冻结授权候选为空',
  last_progress_at: empty.observed_at,
  last_checked_at: null,
  historical_risk: false,
  version: 'v1',
  owner: 'admin',
  next_action: '查看具体校验原因；不自动重放旧消息',
}
function show() {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MobileDispatchRecords />
    </QueryClientProvider>,
  )
}
beforeEach(() => {
  vi.resetAllMocks()
  window.history.replaceState(null, '', '/admin?view=mobile-dispatch-records')
  vi.mocked(dispatchRecords.list).mockResolvedValue({ ...empty, items: [intake] })
  vi.mocked(dispatchRecords.detail).mockResolvedValue({ ...intake, content: '冻结正文' })
  vi.mocked(dispatchRecords.operations).mockResolvedValue(empty)
  vi.mocked(dispatchRecords.events).mockResolvedValue(empty)
  vi.mocked(dispatchRecords.exchanges).mockResolvedValue(empty)
  vi.mocked(dispatchRecords.checks).mockResolvedValue(empty)
})
afterEach(cleanup)
it('shows intake failure without a task or phone receipt and opens its independent history', async () => {
  show()
  expect(await screen.findByText('冻结授权候选为空')).toBeTruthy()
  expect(screen.getByText('尚未创建')).toBeTruthy()
  expect(screen.getByText('暂无落盘接收确认')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '完整下发记录' }))
  expect(await screen.findByText('每次创建处理与调度评估')).toBeTruthy()
  await waitFor(() =>
    expect(dispatchRecords.events).toHaveBeenCalledWith('intake-1', expect.anything()),
  )
  expect(screen.queryByRole('button', { name: '重新发送' })).toBeNull()
})
it('filters reasons on the server and follows the backend cursor', async () => {
  vi.mocked(dispatchRecords.list).mockResolvedValue({
    ...empty,
    items: [intake],
    has_more: true,
    next_cursor: 'cursor-1',
  })
  show()
  await screen.findByText('冻结授权候选为空')
  fireEvent.change(screen.getByLabelText('原因筛选'), { target: { value: 'device_busy' } })
  fireEvent.keyDown(screen.getByLabelText('原因筛选'), { key: 'Enter', code: 'Enter' })
  await waitFor(() =>
    expect(dispatchRecords.list).toHaveBeenCalledWith(
      expect.objectContaining({ reason: 'device_busy' }),
    ),
  )
  await waitFor(() =>
    expect(
      (screen.getAllByRole('button', { name: '下一页' })[0] as HTMLButtonElement).disabled,
    ).toBe(false),
  )
  fireEvent.click(screen.getAllByRole('button', { name: '下一页' })[0])
  await waitFor(() =>
    expect(dispatchRecords.list).toHaveBeenCalledWith(
      expect.objectContaining({ cursor: 'cursor-1' }),
    ),
  )
})
it('opens pre-task intake deep links and reports read errors as errors', async () => {
  window.history.replaceState(null, '', '/admin?view=mobile-dispatch-records&intake=intake-1')
  vi.mocked(dispatchRecords.detail).mockRejectedValue(new Error('服务暂不可用'))
  show()
  expect(await screen.findByText('服务暂不可用')).toBeTruthy()
  expect(screen.getAllByText('记录读取失败，不能据此判断没有下发记录').length).toBeGreaterThan(0)
})

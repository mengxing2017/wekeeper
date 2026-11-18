import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import StatisticsOverview from './index'
import { messageStatistics, businessDirectory, type StatisticsOverview as Overview } from './api'
vi.mock('../../../shared/charts/echart', () => ({ EChart: () => <div aria-label="图表" /> }))
vi.mock('./api', () => ({
  messageStatistics: { create: vi.fn(), details: vi.fn(), export: vi.fn() },
  businessDirectory: { platforms: vi.fn(), units: vi.fn() },
  receiptReasonLabels: {},
}))
const zero = { total: '0', sent: '0', failed: '0', unknown: '0', pending: '0' }
const fixture: Overview = {
  id: 'snapshot',
  metric_version: 'message-statistics/1',
  generated_at: '2026-11-18T02:00:00Z',
  expires_at: '2026-11-18T02:30:00Z',
  timezone: 'Asia/Shanghai',
  basis: 'cohort',
  filter: {
    start: '2026-11-01',
    end: '2026-11-18',
    grain: 'day',
    platform_id: null,
    business_unit_id: null,
    route_id: null,
    source_kind: 'all',
    channel: null,
    client_kind: null,
  },
  summary: {
    received: '1',
    dispatched: '1',
    not_dispatched: '0',
    partial_channel_failure: '0',
    processing: '1',
    incomplete: '0',
    historical_risk: '0',
    targets: { ...zero, total: '1', pending: '1' },
    responsibilities: { ...zero, total: '3', pending: '3' },
    attention: { ...zero, total: '1', pending: '1' },
  },
  cards: [
    {
      key: 'day',
      start: '2026-11-17T16:00:00Z',
      end: '2026-11-18T02:00:00Z',
      comparison_start: '2026-11-16T16:00:00Z',
      comparison_end: '2026-11-17T02:00:00Z',
      count: '1',
      previous: '0',
    },
  ],
  trend: [
    {
      start: '2026-11-17T16:00:00Z',
      received: '1',
      targets: { ...zero, total: '1', pending: '1' },
    },
  ],
  channels: [],
  units: [],
  failures: [],
  unclassified: '0',
}
class ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', ResizeObserver)
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: () => ({
    matches: false,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }),
})
afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})
function show(entry = '/admin?view=statistics-overview') {
  vi.mocked(businessDirectory.platforms).mockResolvedValue({ items: [], has_more: false })
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={cache}>
      <MemoryRouter initialEntries={[entry]}>
        <StatisticsOverview />
      </MemoryRouter>
    </QueryClientProvider>,
  )
}
it('keeps targets and independent phone responsibilities separate and drills into the same version', async () => {
  vi.mocked(messageStatistics.create).mockResolvedValue(fixture)
  vi.mocked(messageStatistics.details).mockResolvedValue({
    items: [],
    has_more: false,
    snapshot_id: 'snapshot',
  })
  show()
  expect(await screen.findByText('应执行责任 3 项')).toBeTruthy()
  expect(screen.getByText('群目标 1 项；成功/全部目标 0%，成功/明确结果 —')).toBeTruthy()
  fireEvent.click(screen.getByText('处理中 3').closest('button')!)
  await waitFor(() =>
    expect(messageStatistics.details).toHaveBeenCalledWith(
      'snapshot',
      expect.objectContaining({ object: 'responsibility', outcome: 'pending', page: 1 }),
      expect.anything(),
    ),
  )
  expect(
    screen.getByText('与图表使用同一查询版本。原记录展示最新业务状态，可能较统计时间更新。'),
  ).toBeTruthy()
})
it('shows query failure without treating it as zero messages', async () => {
  vi.mocked(messageStatistics.create).mockRejectedValue(new Error('数据库暂不可用'))
  show()
  expect(await screen.findByText('统计查询失败：数据库暂不可用')).toBeTruthy()
  expect(screen.queryByText('应执行责任 0 项')).toBeNull()
  expect(screen.getByRole('button', { name: '刷新统计' })).toBeTruthy()
})

it('waits for a matching channel snapshot before drilling down and preserves the result identity', async () => {
  const channel = {
    id: 'mobile:wechat',
    name: 'mobile:wechat',
    platform_name: null,
    received: '1',
    targets: { ...zero, total: '1', sent: '1' },
    responsibilities: { ...zero, total: '3', sent: '3' },
  }
  let finish: (value: Overview) => void = () => {}
  vi.mocked(messageStatistics.create)
    .mockResolvedValueOnce({ ...fixture, channels: [channel] })
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
  vi.mocked(messageStatistics.details).mockResolvedValue({
    items: [],
    has_more: false,
    snapshot_id: 'channel-snapshot',
  })
  show('/admin?view=statistics-overview&stats_client_kind=wechat')
  await screen.findByText('应执行责任 3 项')
  const summaries = screen.getAllByText('查看数据表（精确数值与明细操作）')
  fireEvent.click(summaries.at(-1)!)
  const details = summaries.at(-1)!.closest('details')!
  details.open = true
  fireEvent(details, new Event('toggle'))
  await waitFor(() => expect(details.querySelector('table')).not.toBeNull())
  fireEvent.click(
    Array.from(details.querySelectorAll('button')).find((b) => b.textContent === '1')!,
  )
  await waitFor(() =>
    expect(messageStatistics.create).toHaveBeenCalledWith(
      expect.objectContaining({ channel: 'mobile', client_kind: 'wechat' }),
      expect.anything(),
    ),
  )
  expect(messageStatistics.details).not.toHaveBeenCalled()
  finish({
    ...fixture,
    id: 'channel-snapshot',
    filter: { ...fixture.filter, channel: 'mobile', client_kind: 'wechat' },
    channels: [channel],
  })
  await waitFor(() =>
    expect(messageStatistics.details).toHaveBeenCalledWith(
      'channel-snapshot',
      expect.objectContaining({ object: 'target', outcome: 'sent' }),
      expect.anything(),
    ),
  )
})

import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { DispatchWorkspace } from './index'
import { api } from '../api'
vi.mock('../api', () => ({
  api: {
    targets: vi.fn(),
    groups: vi.fn(),
    dispatch: {
      assignments: vi.fn(),
      releases: vi.fn(),
      groups: vi.fn(),
      batches: vi.fn(),
      batch: vi.fn(),
      failures: vi.fn(),
      enabled: vi.fn(),
      preview: vi.fn(),
      confirm: vi.fn(),
    },
  },
}))
afterEach(() => {
  cleanup()
  vi.resetAllMocks()
})
function show(view: 'devices' | 'compose') {
  vi.mocked(api.targets).mockResolvedValue({ devices: [], accounts: [] } as never)
  vi.mocked(api.groups).mockResolvedValue({
    items: [{ id: 'g1', name: '机器人测试', identity_mode: 'ui_visible', enabled: true }],
  } as never)
  vi.mocked(api.dispatch.groups).mockResolvedValue({ items: [] })
  vi.mocked(api.dispatch.releases).mockResolvedValue({ items: [] })
  vi.mocked(api.dispatch.assignments).mockResolvedValue({
    items: [
      {
        id: 'a1',
        device_name: '离线手机',
        account_name: '业务账号',
        flow_name: '通用群消息',
        enabled: true,
        online: false,
        protocol: 2,
        flow: {
          configuration: {
            policy: { minimum_group_interval_ms: 30000 },
            baseline: { human_group_min_interval_ms: 60000 },
          },
        },
      },
    ],
  } as never)
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <DispatchWorkspace school="s1" view={view} openTask={() => {}} />
    </QueryClientProvider>,
  )
}
it('requires explicit historical review to enable the new school switch', async () => {
  show('devices')
  const enable = await screen.findByRole('button', { name: '启用学校 v5 执行' })
  expect(enable.hasAttribute('disabled')).toBe(true)
  const review = screen.getByRole('checkbox', { name: /已核对本校历史/ })
  fireEvent.click(review)
  expect(enable.hasAttribute('disabled')).toBe(false)
  fireEvent.click(enable)
  await waitFor(() =>
    expect(api.dispatch.enabled).toHaveBeenCalledWith('s1', true, expect.any(String)),
  )
})
it('permits selecting an offline older executor and enforces the human minimum before preview', async () => {
  show('compose')
  fireEvent.mouseDown(screen.getByRole('combobox', { name: '执行范围' }))
  fireEvent.click(await screen.findByText(/离线手机 \/ 业务账号/))
  fireEvent.click(screen.getByRole('button', { name: '填写消息' }))
  await screen.findByText('有效人工下限：60 秒；可延长。')
  fireEvent.change(screen.getByRole('textbox', { name: '发送正文' }), { target: { value: '测试' } })
  fireEvent.mouseDown(screen.getByRole('combobox', { name: '有序群-a1' }))
  fireEvent.click(await screen.findByText('机器人测试'))
  const preview = screen.getByRole('button', { name: '生成核对预览' })
  expect(preview.hasAttribute('disabled')).toBe(true)
  expect(api.dispatch.preview).not.toHaveBeenCalled()
  fireEvent.change(screen.getByRole('spinbutton', { name: '群间等待秒数' }), {
    target: { value: '90' },
  })
  expect(preview.hasAttribute('disabled')).toBe(false)
  vi.mocked(api.dispatch.preview).mockRejectedValue(new Error('preview captured'))
  fireEvent.click(preview)
  await waitFor(() =>
    expect(api.dispatch.preview).toHaveBeenCalledWith(
      's1',
      {
        name: '企业微信群消息',
        text_content: '测试',
        interval_seconds: 90,
        targets: [{ assignment_id: 'a1', group_ids: ['g1'] }],
      },
      expect.any(String),
    ),
  )
  expect(api.dispatch.confirm).not.toHaveBeenCalled()
})

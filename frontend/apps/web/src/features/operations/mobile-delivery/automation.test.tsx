import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ApplicationSetup } from './setup'
import { BusinessWizard } from './flows'
import { api, type AutomationPreview, type CapabilityResolution } from './api'
vi.mock('./api', () => ({
  api: {
    targets: vi.fn(),
    groups: vi.fn(),
    automation: {
      apps: vi.fn(),
      bindings: vi.fn(),
      applications: vi.fn(),
      validate: vi.fn(),
      resolution: vi.fn(),
      preview: vi.fn(),
      confirm: vi.fn(),
    },
  },
}))
class Resize {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', Resize)
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
function show(element: React.ReactNode) {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      {element}
    </QueryClientProvider>,
  )
}
afterEach(() => {
  cleanup()
  vi.resetAllMocks()
})
const app = {
  id: 'app',
  app_key: 'wecom',
  name: '企业微信',
  enabled: true,
  capabilities: [
    {
      id: 'cap',
      name: '向群顺序发送纯文本',
      capability_key: 'group_text',
      minimum_protocol: 4 as const,
    },
  ],
}
const binding = {
  id: 'binding',
  revision: 1,
  school_id: 'school',
  device_id: 'phone',
  app_id: 'app',
  sender_account_id: 'account',
  account_binding_revision: 1,
  instance_ref: 'instance',
  environment_id: 'env',
  device_name: '工作手机',
  app_name: '企业微信',
  account_name: '管理员',
}
it('old phone setup explains waiting without accepting or launching any operation', async () => {
  vi.mocked(api.targets).mockResolvedValue({
    id: 'school',
    devices: [
      {
        id: 'phone',
        name: '工作手机',
        enabled: true,
        revision: 1,
        execution_protocol_version: 3,
        credential_generation: 1,
        client_instance_id: 'phone',
        last_seen_at: null,
        created_at: '',
        sender_account_id: 'account',
        binding_revision: 1,
      },
    ],
    accounts: [],
    devices_has_more: false,
    accounts_has_more: false,
  })
  vi.mocked(api.automation.apps).mockResolvedValue({
    items: [
      app,
      { id: 'wechat', app_key: 'wechat', name: '微信', enabled: false, capabilities: [] },
    ],
  })
  vi.mocked(api.automation.bindings).mockResolvedValue({ items: [] })
  vi.mocked(api.automation.applications).mockResolvedValue({
    id: 'phone',
    status: 'waiting_environment',
    environment: null,
    next_step: '等待支持 v4 的新版 Otter 检查手机环境；当前手机端尚未支持时请勿反复配对',
  })
  show(<ApplicationSetup school="school" />)
  const next = await screen.findByRole('button', { name: '下一步：检查手机' })
  await waitFor(() => expect(next.hasAttribute('disabled')).toBe(false))
  fireEvent.click(next)
  expect(await screen.findByText('等待手机检查')).toBeTruthy()
  expect(screen.getByText(/等待支持 v4 的新版 Otter/)).toBeTruthy()
  expect(screen.getByRole('button', { name: '下一步：测试打开' }).hasAttribute('disabled')).toBe(
    true,
  )
  expect(screen.getByRole('button', { name: '请手机重新检测' }).hasAttribute('disabled')).toBe(true)
  expect(api.automation.validate).not.toHaveBeenCalled()
  expect(screen.queryByRole('textbox')).toBeNull()
})
it('business wizard converts minutes, preserves ordered targets and reuses confirmation key after a lost response', async () => {
  vi.mocked(api.automation.apps).mockResolvedValue({ items: [app] })
  vi.mocked(api.automation.bindings).mockResolvedValue({ items: [binding] })
  vi.mocked(api.automation.resolution).mockResolvedValue({
    configuration: {
      policy: { minimum_group_interval_ms: 60000, default_group_interval_ms: 120000 },
      baseline: { human_group_min_interval_ms: 90000 },
    },
  } as CapabilityResolution)
  vi.mocked(api.groups).mockResolvedValue({
    items: [
      {
        id: 'g1',
        name: '机器人测试',
        enabled: true,
        identity_mode: 'ui_visible',
        verification_state: 'reviewed',
        revision: 1,
      },
      {
        id: 'g2',
        name: '第二个群',
        enabled: true,
        identity_mode: 'ui_visible',
        verification_state: 'reviewed',
        revision: 1,
      },
    ],
  } as Awaited<ReturnType<typeof api.groups>>)
  const preview = {
    id: 'preview',
    revision: 1,
    preview_digest: 'digest',
    expires_at: '2099-01-01T00:00:00Z',
    input: { interval_seconds: 180, text_content: '测试' },
  } as AutomationPreview
  vi.mocked(api.automation.preview).mockResolvedValue(preview)
  vi.mocked(api.automation.confirm)
    .mockRejectedValueOnce(new Error('响应丢失'))
    .mockResolvedValueOnce({ task_id: 'task' } as Awaited<
      ReturnType<typeof api.automation.confirm>
    >)
  const open = vi.fn()
  show(<BusinessWizard school="school" openTask={open} />)
  const next = screen.getByRole('button', { name: '下一步：内容与目标' })
  await waitFor(() => expect(next.hasAttribute('disabled')).toBe(false))
  fireEvent.click(next)
  fireEvent.change(screen.getByLabelText('业务消息正文'), { target: { value: '测试' } })
  const select = screen.getByRole('combobox', { name: '业务目标群' })
  fireEvent.mouseDown(select)
  fireEvent.click(
    await screen.findByText('机器人测试', { selector: '.ant-select-item-option-content' }),
  )
  fireEvent.click(
    await screen.findByText('第二个群', { selector: '.ant-select-item-option-content' }),
  )
  fireEvent.click(screen.getByRole('button', { name: '下一步：节奏与安排' }))
  fireEvent.change(screen.getByRole('spinbutton', { name: '群间等待数值' }), {
    target: { value: '3' },
  })
  fireEvent.click(screen.getByRole('button', { name: '生成服务端预览' }))
  await waitFor(() =>
    expect(api.automation.preview).toHaveBeenCalledWith(
      'school',
      expect.objectContaining({
        app_binding_id: 'binding',
        capability_id: 'cap',
        group_ids: ['g1', 'g2'],
        interval_seconds: 180,
        text_content: '测试',
        schedule: 'manual_once',
      }),
      expect.any(String),
    ),
  )
  const confirm = await screen.findByRole('button', { name: '确认本次业务' })
  expect(confirm.hasAttribute('disabled')).toBe(true)
  fireEvent.click(screen.getByRole('checkbox'))
  fireEvent.click(confirm)
  await screen.findByText('响应丢失')
  fireEvent.click(confirm)
  await waitFor(() => expect(open).toHaveBeenCalledWith('task'))
  const calls = vi.mocked(api.automation.confirm).mock.calls
  expect(calls).toHaveLength(2)
  expect(calls[0]).toEqual(calls[1])
})

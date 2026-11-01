import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { api, sharedSend, type SharedPreview } from './api'
import { SharedSendTasks } from './shared-send-tasks'
vi.mock('./api', () => ({ api: { targets: vi.fn(), bindings: vi.fn() }, sharedSend: { overview: vi.fn(), draft: vi.fn(), preview: vi.fn(), confirm: vi.fn(), task: vi.fn() } }))
afterEach(() => cleanup())
const message = '  第一行\n第二行🙂  '
const preview: SharedPreview = { id: 'preview', digest: 'digest', expires_at: '2099-01-01T00:00:00Z', plan: { schema: 'shared_mobile_send/1', school_id: 'school', draft_id: 'draft', draft_revision: 1, intent_id: 'intent', message, content_digest: 'hash', policy_id: null, items: [{ position: 0, target_id: 'target', selected: null, candidates: [], blocker: 'inspection_required' }] } }
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(api.targets).mockResolvedValue({ items: [{ id: 'target', school_id: 'school', alias: '家长通知', search_text: '实际搜索文字', remark: '', enabled: true, archived_at: null, metadata_revision: 1, locator_revision: 1 }] })
  vi.mocked(sharedSend.overview).mockResolvedValue({ id: 'school', tasks: [], drafts: [], readiness: [], pool: [], inspections: [] })
  vi.mocked(sharedSend.draft).mockResolvedValue({ id: 'draft', school_id: 'school', intent_id: 'intent', target_ids: ['target'], message, content_type: 'text/plain', revision: 1, content_digest: 'hash' })
  vi.mocked(sharedSend.preview).mockResolvedValue(preview)
})
async function compose(admin = true) {
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><SharedSendTasks school="school" admin={admin} /></QueryClientProvider>)
  fireEvent.mouseDown(screen.getByRole('combobox'))
  fireEvent.click(await screen.findByText('家长通知'))
  fireEvent.change(screen.getByLabelText('发送文字'), { target: { value: message } })
}
it('正文保持空格与换行，未检查目标不能确认发送', async () => {
  await compose()
  fireEvent.click(screen.getByRole('button', { name: '保存草稿' }))
  await waitFor(() => expect((screen.getByRole('button', { name: '预览逐群执行计划' }) as HTMLButtonElement).disabled).toBe(false))
  expect(vi.mocked(sharedSend.draft).mock.calls[0][1].message).toBe(message)
  fireEvent.click(screen.getByRole('button', { name: '预览逐群执行计划' }))
  await screen.findByText('等待绑定及自动检查就绪，请查看目标群的具体状态')
  fireEvent.click(screen.getByRole('checkbox'))
  expect((screen.getByRole('button', { name: '确认进入共享发送队列' }) as HTMLButtonElement).disabled).toBe(true)
  expect(sharedSend.confirm).not.toHaveBeenCalled()
})
it('草稿响应不明时重试保持相同内容与幂等键', async () => {
  vi.mocked(sharedSend.draft).mockRejectedValue(new Error('网络响应不明'))
  await compose()
  fireEvent.click(screen.getByRole('button', { name: '保存草稿' }))
  await screen.findByText('网络响应不明')
  await waitFor(() => expect((screen.getByRole('button', { name: '保存草稿' }) as HTMLButtonElement).disabled).toBe(false))
  fireEvent.click(screen.getByRole('button', { name: '保存草稿' }))
  await waitFor(() => expect(sharedSend.draft).toHaveBeenCalledTimes(2))
  expect(vi.mocked(sharedSend.draft).mock.calls[0]).toEqual(vi.mocked(sharedSend.draft).mock.calls[1])
})

it('已绑定但未授权的手机显示本机操作路径并保持禁止发送', async () => {
 const pending:SharedPreview=structuredClone(preview)
 pending.plan.items[0].readiness=[{binding_id:'binding',device_id:'bound-phone',target_id:'target',state:'waiting_initial_consent',ready:false,automatic:true,inspection:null,blockers:[{code:'waiting_initial_consent',message:'等待首次授权',owner:'device',automatic:false},{code:'acceptance_missing',message:'该范围尚未完成真机验收放行',owner:'engineering',automatic:false}]}]
 vi.mocked(sharedSend.preview).mockResolvedValue(pending)
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'保存草稿'}))
 await waitFor(()=>expect((screen.getByRole('button',{name:'预览逐群执行计划'}) as HTMLButtonElement).disabled).toBe(false))
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 expect(await screen.findByText(/不会因 Web 预览自动弹窗/)).toBeTruthy()
 expect(screen.getByText('已绑定手机：bound-phone')).toBeTruthy()
 expect(screen.getByText('平台维护人员：该范围尚未完成真机验收放行')).toBeTruthy()
 expect(screen.queryByText('无可用设备')).toBeNull()
 fireEvent.click(screen.getByRole('checkbox'))
 expect((screen.getByRole('button',{name:'确认进入共享发送队列'}) as HTMLButtonElement).disabled).toBe(true)
 expect(sharedSend.confirm).not.toHaveBeenCalled()
})

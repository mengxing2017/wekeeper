import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { api, sharedSend, type SharedPreview } from './api'
import { SharedSendTasks } from './shared-send-tasks'
vi.mock('./api', () => ({ api: { targets: vi.fn(), bindings: vi.fn() }, sharedSend: { overview: vi.fn(), taskPreview: vi.fn(), confirm: vi.fn(), task: vi.fn() } }))
afterEach(() => cleanup())
const response=(p:SharedPreview)=>({id:p.id,draft:{id:'draft',targets:[{id:'target',name:'家长通知'}],school_id:'school',intent_id:'intent',target_ids:['target'],message,content_type:'text/plain' as const,stage:'production_allowlist' as const,between_targets_seconds:30,revision:1,content_digest:'hash'},preview:p})
const message = '  第一行\n第二行🙂  '
const preview: SharedPreview = { id: 'preview', digest: 'digest', expires_at: '2099-01-01T00:00:00Z', plan: { schema: 'shared_mobile_send/2', school_id: 'school', draft_id: 'draft', draft_revision: 1, intent_id: 'intent', message, content_digest: 'hash', policy_id: null, items: [{ position: 0, target_id: 'target', target:{id:'target',name:'家长通知',search_text:'实际搜索文字',locator_revision:1},selected: null, candidates: [], blocker: 'inspection_required' }] } }
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(api.targets).mockResolvedValue({ items: [{ id: 'target', school_id: 'school', alias: '家长通知', search_text: '实际搜索文字', remark: '', enabled: true, archived_at: null, metadata_revision: 1, locator_revision: 1 }] })
  vi.mocked(sharedSend.overview).mockResolvedValue({ id: 'school', tasks: [], drafts: [], readiness: [], pool: [], inspections: [] })
  vi.mocked(sharedSend.taskPreview).mockImplementation(async()=>({id:preview.id,draft:{id:'draft',targets:[{id:'target',name:'家长通知'}],school_id:'school',intent_id:'intent',target_ids:['target'],message,content_type:'text/plain',stage:'production_allowlist',between_targets_seconds:30,revision:1,content_digest:'hash'},preview}))
})
async function compose(admin = true) {
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><SharedSendTasks school="school" admin={admin} /></QueryClientProvider>)
  fireEvent.mouseDown(screen.getByLabelText('手机目标群'))
  fireEvent.click(await screen.findByText('家长通知'))
  fireEvent.change(screen.getByLabelText('发送文字'), { target: { value: message } })
}
it('正文保持空格与换行，未检查目标不能确认发送', async () => {
  await compose()
  expect(screen.queryByRole('button',{name:'保存草稿'})).toBeNull();expect(screen.queryByLabelText('发送阶段')).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: '预览逐群执行计划' }))
  await screen.findByText('等待绑定及自动检查就绪，请查看目标群的具体状态')
  expect((screen.getByRole('button', { name: '确认发送' }) as HTMLButtonElement).disabled).toBe(true)
  expect(sharedSend.confirm).not.toHaveBeenCalled()
})

it('已绑定但未授权的手机显示本机操作路径并保持禁止发送', async () => {
 const pending:SharedPreview=structuredClone(preview)
 pending.plan.items[0].readiness=[{device_name:'现场共享手机',target_name:'家长通知',search_text:'实际搜索文字',inspection_state:'waiting_prerequisites',retry_at:null,retry_remaining:3,binding_id:'binding',device_id:'bound-phone',target_id:'target',state:'waiting_initial_consent',ready:false,plan_selectable:true,confirmable:false,execution_ready:false,confirmable_stages:[],automatic:true,inspection:null,blockers:[{code:'waiting_initial_consent',message:'等待首次授权',owner:'device',automatic:false},{code:'acceptance_missing',message:'该范围尚未完成真机验收放行',owner:'engineering',automatic:false}]}]
 vi.mocked(sharedSend.taskPreview).mockResolvedValue(response(pending))
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 expect(await screen.findByText(/不会因 Web 预览自动弹窗/)).toBeTruthy()
 expect(screen.getByText('已绑定手机：现场共享手机')).toBeTruthy()
 expect(screen.getByText('系统支持状态：当前手机应用暂不支持执行，请在使用管理中报告问题。')).toBeTruthy()
 expect(screen.queryByText('无可用设备')).toBeNull()
 expect((screen.getByRole('button',{name:'确认发送'}) as HTMLButtonElement).disabled).toBe(true)
 expect(sharedSend.confirm).not.toHaveBeenCalled()
})

it('无手机候选仍显示群名和搜索文字，重新预览生成新操作并清除确认',async()=>{
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await screen.findByText('等待绑定及自动检查就绪，请查看目标群的具体状态')
 expect(screen.getAllByText('家长通知').length).toBeGreaterThanOrEqual(2)
 expect(screen.getByRole('cell',{name:'实际搜索文字'})).toBeTruthy()
 expect(screen.queryByRole('cell',{name:'target'})).toBeNull()
 const phone:NonNullable<SharedPreview['plan']['items'][number]['selected']>={target_id:'target',target_name:'家长通知',device_name:'新就绪手机',device_id:'phone',binding_id:'binding',binding_locator_revision:1,target_locator_revision:1,search_text:'实际搜索文字',search_source:'default',access_revision:null,credential_generation:1,delegation_id:'delegation',release_id:'release',flow_digest:'digest',flow:{}}
 const next=structuredClone(preview);next.id='new-preview';next.expires_at='2099-02-01T00:00:00Z'
 next.plan.items[0]={...next.plan.items[0],selected:phone,candidates:[phone],blocker:null,readiness:[{binding_id:'binding',device_id:'phone',target_id:'target',device_name:'新就绪手机',target_name:'家长通知',search_text:'实际搜索文字',state:'ready_for_task',ready:true,plan_selectable:true,confirmable:true,execution_ready:true,confirmable_stages:["test_send"],automatic:true,blockers:[],inspection:null,inspection_state:'passed',retry_at:null,retry_remaining:3}]}
 vi.mocked(sharedSend.taskPreview).mockResolvedValue(response(next))
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await screen.findByText(/2099-02-01T00:00:00Z/)
 expect(await screen.findByText('新就绪手机')).toBeTruthy()
 expect(vi.mocked(sharedSend.taskPreview).mock.calls[0][2]).not.toBe(vi.mocked(sharedSend.taskPreview).mock.calls[1][2])
 expect(screen.queryByRole('checkbox')).toBeNull()
},60000)

it('预览响应不明保持原键，成功后的主动预览使用新键',async()=>{
 vi.mocked(sharedSend.taskPreview).mockRejectedValueOnce(new Error('预览响应不明')).mockResolvedValue(response(preview))
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await screen.findByText('预览响应不明')
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await screen.findByText('等待绑定及自动检查就绪，请查看目标群的具体状态')
 const calls=vi.mocked(sharedSend.taskPreview).mock.calls
 expect(calls[0][2]).toBe(calls[1][2])
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await waitFor(()=>expect(sharedSend.taskPreview).toHaveBeenCalledTimes(3))
 expect(calls[2][2]).not.toBe(calls[1][2])
},60000)


it('离线已绑定手机可选且静态范围有效时可确认等待，不发起手机检查',async()=>{
 const pending=structuredClone(preview)
 const phone={target_id:'target',target_name:'家长通知',device_name:'离线测试手机',device_id:'phone',binding_id:'binding',binding_locator_revision:1,target_locator_revision:1,search_text:'实际搜索文字',search_source:'default' as const,access_revision:null,credential_generation:1,delegation_id:null,release_id:'release',flow_digest:'digest',flow:{},confirmable:true}
 pending.plan.items[0]={...pending.plan.items[0],selected:phone,candidates:[phone],blocker:null,readiness:[{binding_id:'binding',device_id:'phone',target_id:'target',device_name:'离线测试手机',target_name:'家长通知',search_text:'实际搜索文字',state:'waiting_connection',ready:false,plan_selectable:true,confirmable:true,execution_ready:false,confirmable_stages:['test_send'],automatic:true,blockers:[{code:'waiting_connection',message:'手机离线，连接后自动继续',owner:'system',automatic:true}],inspection:null,inspection_state:'waiting_prerequisites',retry_at:null,retry_remaining:3}]}
 vi.mocked(sharedSend.taskPreview).mockResolvedValue(response(pending))
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 const select=await screen.findByLabelText('目标群 target 的共享手机')
 fireEvent.mouseDown(select)
 expect(document.querySelector('.ant-select-item-option-disabled')).toBeNull()
 expect((screen.getByRole('button',{name:'确认发送'}) as HTMLButtonElement).disabled).toBe(false)
 expect(sharedSend.confirm).not.toHaveBeenCalled()
},120000)

import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { api, sharedSend, type SharedPreview, type SharedCandidate } from './api'
import { SharedSendTasks } from './shared-send-tasks'
vi.mock('./api', () => ({ api: { targets: vi.fn(), bindings: vi.fn() }, sharedSend: { overview: vi.fn(), taskPreview: vi.fn(), confirm: vi.fn(), task: vi.fn() } }))
afterEach(() => {cleanup();vi.useRealTimers()})
const response=(p:SharedPreview)=>({id:p.id,draft:{id:'draft',targets:[{id:'target',name:'实际搜索文字'}],school_id:'school',intent_id:'intent',target_ids:['target'],message,content_type:'text/plain' as const,between_targets_seconds:30,revision:1,content_digest:'hash'},preview:p})
const message = '  第一行\n第二行🙂  '
const preview: SharedPreview = { id: 'preview', digest: 'digest', expires_at: '2099-01-01T00:00:00Z', plan: { schema: 'shared_mobile_send/2', school_id: 'school', draft_id: 'draft', draft_revision: 1, intent_id: 'intent', message, content_digest: 'hash', policy_id: null, items: [{ position: 0, target_id: 'target', target:{id:'target',name:'实际搜索文字',search_text:'实际搜索文字',locator_revision:1},selected: null, candidates: [], blocker: 'inspection_required' }] } }
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(sharedSend.task).mockResolvedValue({id:'task',school_id:'school',intent_id:'intent',revision:1,stopped:false,created_at:'2026-11-03'})
  vi.mocked(api.targets).mockResolvedValue({ items: [{ id: 'target', school_id: 'school', search_text: '实际搜索文字', remark: '', enabled: true, archived_at: null, metadata_revision: 1, locator_revision: 1 }] })
  vi.mocked(sharedSend.overview).mockResolvedValue({ id: 'school', tasks: [], drafts: [], readiness: [], pool: [], inspections: [] })
  vi.mocked(sharedSend.taskPreview).mockImplementation(async()=>({id:preview.id,draft:{id:'draft',targets:[{id:'target',name:'实际搜索文字'}],school_id:'school',intent_id:'intent',target_ids:['target'],message,content_type:'text/plain',between_targets_seconds:30,revision:1,content_digest:'hash'},preview}))
})
async function compose(admin = true) {
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><SharedSendTasks school="school" admin={admin} /></QueryClientProvider>)
  fireEvent.mouseDown(screen.getByLabelText('手机目标群'))
  fireEvent.click(await screen.findByText('实际搜索文字'))
  fireEvent.change(screen.getByLabelText('发送文字'), { target: { value: message } })
}
it('本校管理员可点击确认，预览检查问题由提交接口明确反馈', async () => {
  vi.mocked(sharedSend.confirm).mockRejectedValue(new Error('请选择已绑定手机'))
  await compose()
  expect(screen.queryByRole('button',{name:'保存草稿'})).toBeNull();expect(screen.queryByLabelText('发送阶段')).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: '预览逐群执行计划' }))
  await screen.findByText('没有可选绑定，请核对本校目标群与手机的关联')
  const button=screen.getByRole('button', { name: '确认发送' }) as HTMLButtonElement
  expect(button.disabled).toBe(false)
  fireEvent.click(button)
  await waitFor(()=>expect(sharedSend.confirm).toHaveBeenCalledOnce())
  const dialog=await screen.findByRole('dialog')
  expect(within(dialog).getAllByText('确认发送失败').length).toBeGreaterThan(0)
  expect(within(dialog).getByText('请选择已绑定手机')).toBeTruthy()
  fireEvent.click(within(dialog).getByRole('button',{name:'知道了'}))
  await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull())
  expect(screen.getByRole('button',{name:'确认发送'})).toBeTruthy()
})

it('手机授权状态未知与状态过期不阻止确认，也不要求重新授权',async()=>{
 const pending=structuredClone(preview)
 const phone:SharedCandidate={target_id:'target',target_name:'实际搜索文字',device_name:'测试手机',device_id:'phone',binding_id:'binding',binding_locator_revision:1,target_locator_revision:1,target:{kind:'group',search_query:'实际搜索文字',match_policy:'exact_unique',identity_constraints:[]},search_source:'default' as const,access_revision:null,credential_generation:1,delegation_id:null,release_id:null,flow_digest:null,flow:null,confirmable:true}
 pending.plan.dispatch_mode='candidate_pool'
 pending.plan.items[0]={...pending.plan.items[0],selected:phone,candidates:[phone],blocker:null,readiness:[{binding_id:'binding',device_id:'phone',target_id:'target',device_name:'测试手机',target_name:'实际搜索文字',search_text:'实际搜索文字',state:'runtime_stale',ready:false,plan_selectable:true,confirmable:true,execution_ready:false,automatic:true,blockers:[],inspection:null,inspection_state:'waiting_prerequisites',retry_at:null,retry_remaining:3,consent_state:'unknown',runtime_state:'stale',execution_conditions:[{code:'runtime_stale',message:'手机执行状态已过期，不要求重新授权',owner:'system',automatic:true}]}]}
 vi.mocked(sharedSend.taskPreview).mockResolvedValue(response(pending))
 vi.mocked(sharedSend.confirm).mockResolvedValue({id:'task',school_id:'school',intent_id:'intent',revision:1,stopped:false,created_at:'2026-11-03'})
 await compose();fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 expect(await screen.findByText('手机托管：状态待同步，不代表未授权')).toBeTruthy()
 expect(screen.getByText('确认的授权候选：测试手机（实际搜索文字）；所选为首选，范围内允许自动换机。')).toBeTruthy()
 expect(screen.queryByText(/undefined/)).toBeNull()
 expect(screen.getByText('执行状态：手机执行状态已过期，不要求重新授权')).toBeTruthy()
 expect(screen.queryByText(/查看范围并启用托管/)).toBeNull()
 const confirm=screen.getByRole('button',{name:'确认发送'}) as HTMLButtonElement
 expect(confirm.disabled).toBe(false);fireEvent.click(confirm)
 await waitFor(()=>expect(sharedSend.confirm).toHaveBeenCalledOnce())
},60000)

it('无手机候选仍显示群名和搜索文字，重新预览生成新操作并清除确认',async()=>{
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await screen.findByText('没有可选绑定，请核对本校目标群与手机的关联')
 expect(screen.getAllByText('实际搜索文字').length).toBeGreaterThanOrEqual(2)
 expect(screen.getByRole('cell',{name:'实际搜索文字'})).toBeTruthy()
 expect(screen.queryByRole('cell',{name:'target'})).toBeNull()
 const phone:NonNullable<SharedPreview['plan']['items'][number]['selected']>={target_id:'target',target_name:'实际搜索文字',device_name:'新就绪手机',device_id:'phone',binding_id:'binding',binding_locator_revision:1,target_locator_revision:1,search_text:'实际搜索文字',search_source:'default',access_revision:null,credential_generation:1,delegation_id:'delegation',release_id:'release',flow_digest:'digest',flow:{}}
 const next=structuredClone(preview);next.id='new-preview';next.expires_at='2099-02-01T00:00:00Z'
 next.plan.items[0]={...next.plan.items[0],selected:phone,candidates:[phone],blocker:null,readiness:[{binding_id:'binding',device_id:'phone',target_id:'target',device_name:'新就绪手机',target_name:'实际搜索文字',search_text:'实际搜索文字',state:'ready_for_task',ready:true,plan_selectable:true,confirmable:true,execution_ready:true,automatic:true,blockers:[],inspection:null,inspection_state:'passed',retry_at:null,retry_remaining:3}]}
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
 await screen.findByText('没有可选绑定，请核对本校目标群与手机的关联')
 const calls=vi.mocked(sharedSend.taskPreview).mock.calls
 expect(calls[0][2]).toBe(calls[1][2])
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await waitFor(()=>expect(sharedSend.taskPreview).toHaveBeenCalledTimes(3))
 expect(calls[2][2]).not.toBe(calls[1][2])
},60000)


it('离线已绑定手机可选且静态范围有效时可确认等待，不发起手机检查',async()=>{
 const pending=structuredClone(preview)
 const phone={target_id:'target',target_name:'实际搜索文字',device_name:'离线测试手机',device_id:'phone',binding_id:'binding',binding_locator_revision:1,target_locator_revision:1,search_text:'实际搜索文字',search_source:'default' as const,access_revision:null,credential_generation:1,delegation_id:null,release_id:'release',flow_digest:'digest',flow:{},confirmable:true}
 pending.plan.items[0]={...pending.plan.items[0],selected:phone,candidates:[phone],blocker:null,readiness:[{binding_id:'binding',device_id:'phone',target_id:'target',device_name:'离线测试手机',target_name:'实际搜索文字',search_text:'实际搜索文字',state:'waiting_connection',ready:false,plan_selectable:true,confirmable:true,execution_ready:false,automatic:true,blockers:[{code:'waiting_connection',message:'手机离线，连接后自动继续',owner:'system',automatic:true}],inspection:null,inspection_state:'waiting_prerequisites',retry_at:null,retry_remaining:3}]}
 vi.mocked(sharedSend.taskPreview).mockResolvedValue(response(pending))
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 const select=await screen.findByLabelText('目标群 target 的共享手机')
 fireEvent.mouseDown(select)
 expect(document.querySelector('.ant-select-item-option-disabled')).toBeNull()
 expect((screen.getByRole('button',{name:'确认发送'}) as HTMLButtonElement).disabled).toBe(false)
 expect(sharedSend.confirm).not.toHaveBeenCalled()
},120000)

it('确认时才发现配置损坏由服务端明确拒绝，并保留输入',async()=>{
 const pending=structuredClone(preview)
 const phone={target_id:'target',target_name:'实际搜索文字',device_name:'离线测试手机',device_id:'phone',binding_id:'binding',binding_locator_revision:1,target_locator_revision:1,search_text:'实际搜索文字',search_source:'default' as const,access_revision:null,credential_generation:1,delegation_id:null,release_id:null,flow_digest:null,flow:null,confirmable:false}
 pending.plan.items[0]={...pending.plan.items[0],selected:phone,candidates:[phone],blocker:'shared_send_execution_configuration_invalid',readiness:[{binding_id:'binding',device_id:'phone',target_id:'target',device_name:'离线测试手机',target_name:'实际搜索文字',search_text:'实际搜索文字',state:'waiting_connection',ready:false,plan_selectable:true,confirmable:false,execution_ready:false,automatic:true,blockers:[{code:'waiting_connection',message:'手机离线，连接后自动继续',owner:'system',automatic:true}],inspection:null,inspection_state:'waiting_prerequisites',retry_at:null,retry_remaining:3}]}
 vi.mocked(sharedSend.taskPreview).mockResolvedValue(response(pending))
 vi.mocked(sharedSend.confirm).mockRejectedValue(Object.assign(new Error('配置异常'),{code:'shared_send_execution_configuration_invalid'}))
 await compose()
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 await screen.findByLabelText('目标群 target 的共享手机')
 expect((screen.getByRole('button',{name:'确认发送'}) as HTMLButtonElement).disabled).toBe(false)
 fireEvent.click(screen.getByRole('button',{name:'确认发送'}))
 const dialog=await screen.findByRole('dialog')
 expect(within(dialog).getByText('当前发送配置异常，本次未执行。可报告问题，修复后重新预览。')).toBeTruthy()
 expect(screen.getByLabelText('发送文字')).toHaveProperty('value',message)
 expect(screen.queryByText('平台已创建，等待手机同步')).toBeNull()
},120000)

it('缺少本校管理员角色时明确说明权限原因，而不是无说明置灰',async()=>{
 await compose(false)
 fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 expect(await screen.findByText('没有本校管理员发送权限')).toBeTruthy()
 expect(screen.getByText('当前账号没有本校管理员发送权限；平台管理员也需在学校成员管理中获得本校管理员角色')).toBeTruthy()
 expect((screen.getByRole('button',{name:'确认发送'}) as HTMLButtonElement).disabled).toBe(true)
 expect(sharedSend.confirm).not.toHaveBeenCalled()
},60000)


it('打开详情持续轮询，失败保留快照与输入，关闭和切校停止旧任务查询',async()=>{
 const task={id:'task',school_id:'school',intent_id:'intent',revision:1,stopped:false,created_at:'2026-11-03',items:[{id:'item',target_id:'target',position:0,attempts:[{id:'attempt',state:'queued',revision:1,current:true,attempt_no:1,plan_digest:'hash',plan:{search_text:'实际搜索文字',device_id:'phone',device_name:'测试手机'},execution_status:{conditions:[{code:'runtime_stale',message:'等待手机上报当前执行条件',owner:'system'}]}}]}]} as unknown as Awaited<ReturnType<typeof sharedSend.task>>
 vi.mocked(sharedSend.overview).mockResolvedValue({id:'school',tasks:[task],drafts:[],readiness:[],pool:[],inspections:[]})
 vi.mocked(sharedSend.task).mockResolvedValue(task)
 const client=new QueryClient({defaultOptions:{queries:{retry:false}}})
 const ui=(school:string)=><QueryClientProvider client={client}><SharedSendTasks school={school} admin/></QueryClientProvider>
 const rendered=render(ui('school'))
 fireEvent.click(await screen.findByRole('button',{name:'查看计划与结果'},{timeout:10_000}))
 await screen.findByText(/等待手机上报当前执行条件/,{},{timeout:10_000})
 expect(sharedSend.task).toHaveBeenCalledTimes(1)
 vi.mocked(sharedSend.task).mockRejectedValueOnce(new Error('模拟断网'))
 await waitFor(()=>expect(screen.getByText('进度读取失败，以下为上次获取的快照')).toBeTruthy(),{timeout:10_000})
 expect(screen.getByText(/等待手机上报当前执行条件/)).toBeTruthy()
 fireEvent.click(screen.getByRole('button',{name:'停止尚未发放许可的任务'}))
 fireEvent.change(screen.getByLabelText('核对依据与操作理由'),{target:{value:'保留待提交理由'}})
 const running=structuredClone(task);running.items![0].attempts[0].state='prepared';running.revision=2
 vi.mocked(sharedSend.task).mockResolvedValue(running)
 await waitFor(()=>expect(screen.getByText(/文字准备完成/)).toBeTruthy(),{timeout:10_000})
 expect((screen.getByLabelText('核对依据与操作理由') as HTMLTextAreaElement).value).toBe('保留待提交理由')
 expect(sharedSend.confirm).not.toHaveBeenCalled()
 fireEvent.click(within(screen.getByLabelText('核对依据与操作理由').closest('[role=dialog]') as HTMLElement).getByRole('button',{name:'Cancel'}))
 fireEvent.click(within(screen.getByRole('button',{name:'停止尚未发放许可的任务'}).closest('[role=dialog]') as HTMLElement).getByRole('button',{name:'Close'}))
 await waitFor(()=>expect(screen.queryByRole('button',{name:'停止尚未发放许可的任务'})).toBeNull())
 const closedCount=vi.mocked(sharedSend.task).mock.calls.length
 await act(async()=>{await new Promise(resolve=>setTimeout(resolve,5_100))})
 expect(sharedSend.task).toHaveBeenCalledTimes(closedCount)
 fireEvent.click(screen.getByRole('button',{name:'查看计划与结果'}))
 await screen.findByText(/文字准备完成/,{},{timeout:10_000})
 fireEvent.click(screen.getByRole('button',{name:'停止尚未发放许可的任务'}))
 fireEvent.change(screen.getByLabelText('核对依据与操作理由'),{target:{value:'切校前的理由'}})
 // Switch school while both dialogs are open; old details and local action state are gone.
 rendered.rerender(ui('another-school'))
 await act(async()=>{await new Promise(resolve=>setTimeout(resolve,5_100))})
 expect(screen.queryByRole('button',{name:'停止尚未发放许可的任务'})).toBeNull()
 expect(screen.queryByLabelText('核对依据与操作理由')).toBeNull()
 const count=vi.mocked(sharedSend.task).mock.calls.length
 await act(async()=>{await new Promise(resolve=>setTimeout(resolve,5_100))})
 expect(sharedSend.task).toHaveBeenCalledTimes(count)
 client.clear()
},60000)


it('排队任务分别显示平台已创建与手机已接收，接收回执不改变执行条件',async()=>{
 const task={id:'task',school_id:'school',intent_id:'intent',revision:1,stopped:false,created_at:'2026-11-04',items:[{id:'item',target_id:'target-uuid',position:0,attempts:[{id:'attempt',state:'queued',revision:1,current:true,attempt_no:1,plan_digest:'hash',plan:{target_name:'冻结群名',search_text:'专用搜索文字',device_id:'phone',device_name:'测试手机'},execution_status:{conditions:[{code:'runtime_stale',message:'等待手机执行条件更新',owner:'system'}]}}]}]} as unknown as Awaited<ReturnType<typeof sharedSend.task>>
 vi.mocked(sharedSend.overview).mockResolvedValue({id:'school',tasks:[task],drafts:[],readiness:[],pool:[],inspections:[]})
 vi.mocked(sharedSend.task).mockResolvedValue(task)
 const client=new QueryClient({defaultOptions:{queries:{retry:false}}})
 render(<QueryClientProvider client={client}><SharedSendTasks school="school" admin/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'查看计划与结果'},{timeout:10_000}))
 expect(await screen.findByText(/平台已创建，等待手机同步/,{},{timeout:10_000})).toBeTruthy()
 expect(screen.getByText(/冻结群名/)).toBeTruthy()
 const received=structuredClone(task);received.items![0].attempts[0].device_received_at='2026-11-04T00:00:00Z'
 act(()=>{client.setQueryData(['mobile-execution','school','shared-task','task'],received)})
 expect(await screen.findByText(/手机已接收，等待执行条件/,{},{timeout:10_000})).toBeTruthy()
 expect(screen.getByText(/等待手机执行条件更新/)).toBeTruthy()
 expect(sharedSend.confirm).not.toHaveBeenCalled()
 client.clear()
},60000)

it('群名变化必须重新预览，手机配置不是受理条件',async()=>{
 const pending=structuredClone(preview)
 const code='plan_dependency_changed'
 const text='目标群名已变化，请重新预览'
 const row={binding_id:'binding',device_id:'phone',target_id:'target',device_name:'测试手机',target_name:'实际搜索文字',search_text:'实际搜索文字',state:code,ready:false,plan_selectable:true,confirmable:false,execution_ready:false,automatic:true,blockers:[{code,message:text,owner:'system',automatic:false}],inspection:null,inspection_state:'not_scheduled' as const,retry_at:null,retry_remaining:3,configuration_id:null,configuration_digest:null,consent_state:'accepted' as const,runtime_state:'screen_off' as const,execution_conditions:[{code:'device_screen_off',message:'手机屏幕已关闭，有有效待办时自动准备亮屏',owner:'device',automatic:true}]}
 pending.plan.items[0]={...pending.plan.items[0],blocker:text,readiness:[row]}
 vi.mocked(sharedSend.taskPreview).mockResolvedValue(response(pending))
 vi.mocked(sharedSend.overview).mockResolvedValue({id:'school',tasks:[],drafts:[],readiness:[{...row,search_text:'新群名'}],pool:[],inspections:[]})
 await compose();fireEvent.click(screen.getByRole('button',{name:'预览逐群执行计划'}))
 expect(await screen.findByText(`受理检查：${text}`)).toBeTruthy()
 expect(screen.queryByText(`受理条件：${text}`)).toBeNull()
 expect(screen.queryByRole('button',{name:'查看本校使用范围'})).toBeNull()
 expect(screen.getByText('业务资源已变化，请重新预览后确认')).toBeTruthy()
 expect((screen.getByRole('button',{name:'确认发送'}) as HTMLButtonElement).disabled).toBe(true)
 expect(sharedSend.confirm).not.toHaveBeenCalled()
},60000)

it('展示授权候选和自动换机轨迹，历史尝试结果冲突仍可核对',async()=>{
 const candidates:SharedCandidate[]=['甲','乙'].map((name,index)=>({target_id:'target',target_name:'显示群名',device_id:`phone-${index}`,device_name:`候选${name}`,binding_id:`binding-${index}`,binding_locator_revision:1,target_locator_revision:1,target:{kind:'group',search_query:`${name}群名`,match_policy:'exact_unique',identity_constraints:[]},search_source:'default',access_revision:null,credential_generation:1,delegation_id:null}))
 const task={id:'task',school_id:'school',intent_id:'intent',dispatch_mode:'candidate_pool',revision:1,stopped:false,created_at:'2026-11-04',items:[{id:'item',target_id:'target',position:0,dispatch:{state:'needs_review',candidates,assignments:[{id:'assignment',source:'automatic',from_device_name:'候选甲',to_device_name:'候选乙',created_at:'2026-11-04T00:00:00Z'}]},attempts:[{id:'old',state:'unknown',current:false,attempt_no:1,revision:3,plan:{...candidates[0],message,between_targets_seconds:30}},{id:'new',state:'queued',current:true,attempt_no:2,revision:1,plan:{...candidates[1],message,between_targets_seconds:30}}]}]} as unknown as Awaited<ReturnType<typeof sharedSend.task>>
 vi.mocked(sharedSend.overview).mockResolvedValue({id:'school',tasks:[task],drafts:[],readiness:[],pool:[],inspections:[]})
 vi.mocked(sharedSend.task).mockResolvedValue(task)
 const client=new QueryClient({defaultOptions:{queries:{retry:false}}})
 render(<QueryClientProvider client={client}><SharedSendTasks school="school" admin/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'查看计划与结果'},{timeout:10_000}))
 expect(await screen.findByText('目标调度：本目标结果待核对，其他目标继续调度',{},{timeout:10_000})).toBeTruthy()
 expect(screen.getByText('授权候选：候选甲（甲群名）、候选乙（乙群名）')).toBeTruthy()
 expect(screen.getByText('目标群：显示群名')).toBeTruthy()
 expect(screen.getByText(/甲群名 \/ 候选甲/)).toBeTruthy()
 expect(screen.getByText(/乙群名 \/ 候选乙/)).toBeTruthy()
 expect(screen.queryByText(/undefined/)).toBeNull()
 expect(screen.getByText(/自动换机：候选甲 → 候选乙/)).toBeTruthy()
 expect(screen.getByText(/平台已受理，按授权候选调度/)).toBeTruthy()
 expect(screen.getByRole('button',{name:'人工核对结果'})).toBeTruthy()
 fireEvent.click(screen.getByRole('button',{name:'复制原意图为新草稿（需重新确认）'}))
 expect(screen.getByText('乙群名')).toBeTruthy()
 expect(screen.getByLabelText('发送文字')).toHaveProperty('value',message)
 expect(sharedSend.confirm).not.toHaveBeenCalled()
 client.clear()
},60000)


it('页面识别失败显示实际原因，授权候选继续处理且不要求版本适配',async()=>{
 const task={id:'task',school_id:'school',intent_id:'intent',dispatch_mode:'candidate_pool',revision:1,stopped:false,created_at:'2026-11-05',items:[{id:'item',target_id:'target',position:0,dispatch:{state:'assigned',candidates:[{device_name:'候选乙',search_text:'乙备注群名'}],assignments:[]},attempts:[{id:'old',state:'failed_before_call',result_reason:'page_unrecognized',current:false,attempt_no:1,revision:3,plan:{search_text:'甲备注群名',device_name:'候选甲'}},{id:'new',state:'queued',current:true,attempt_no:2,revision:1,plan:{search_text:'乙备注群名',device_name:'候选乙'}}]}]} as unknown as Awaited<ReturnType<typeof sharedSend.task>>
 vi.mocked(sharedSend.overview).mockResolvedValue({id:'school',tasks:[task],drafts:[],readiness:[],pool:[],inspections:[]})
 vi.mocked(sharedSend.task).mockResolvedValue(task)
 const client=new QueryClient({defaultOptions:{queries:{retry:false}}})
 render(<QueryClientProvider client={client}><SharedSendTasks school="school" admin/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'查看计划与结果'},{timeout:10_000}))
 expect(await screen.findByText('；本次尝试：手机无法识别当前企业微信页面',{},{timeout:10_000})).toBeTruthy()
 expect(screen.getByText('目标调度：已分配可用候选手机')).toBeTruthy()
 expect(screen.getByText(/平台已受理，按授权候选调度/)).toBeTruthy()
 expect(screen.queryByText(/版本适配|交付处理|排队等待共享手机/)).toBeNull()
 expect(sharedSend.confirm).not.toHaveBeenCalled()
 client.clear()
},60000)

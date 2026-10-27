import {afterEach,expect,it,vi} from 'vitest'
import {configure,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import BroadcastTasks,{targetLabel} from './index'
import {broadcast,workspace,type BroadcastTask,type BroadcastTarget} from './api'
vi.mock('./api',()=>({broadcast:{list:vi.fn(),task:vi.fn(),capability:vi.fn(),targets:vi.fn(),events:vi.fn(),action:vi.fn(),save:vi.fn()},workspace:{list:vi.fn()}}))
vi.mock('../../../shared/use-dialogs',()=>({useDialogs:()=>({confirm:async()=>true,prompt:async()=>'原因',contextHolder:null})}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
configure({asyncUtilTimeout:5000})
afterEach(()=>{cleanup();vi.clearAllMocks()})
const task:BroadcastTask={id:'task-123456',school_id:'school',creator_id:'actor',content_source:'posts',template_version_id:'v',content:'冻结通知',revision:1,state:'draft',stopped:false,created_at:'2026-10-22T00:00:00Z',total:2,sent:0,rejected:0,uncertain:0,pending:2,batch_count:1,post_ids:['p'],source_task_id:null,source_draft_id:null,resend_reason:''}
const target:BroadcastTarget={id:'target',group_id:'g',chat_id:'chat',group_name:'测试群',batch_id:'batch',sender:'owner',creation_state:'created',msgid:'official-task',rejected:false,official_status:null,send_time:null,member_status:2,last_synced_at:null,sync_error:null,error_code:null,stop_state:'none',reconciliation:null,operation_unknown:false}
function show(admin=true, selectedTask:BroadcastTask=task, blockedReason:string|null=null){
 vi.mocked(broadcast.list).mockResolvedValue({items:[selectedTask]});vi.mocked(broadcast.task).mockResolvedValue(selectedTask)
 vi.mocked(broadcast.capability).mockResolvedValue({id:'current',broadcast_available:blockedReason===null,management_available:true,reason:blockedReason})
 vi.mocked(broadcast.targets).mockResolvedValue({items:[target]});vi.mocked(broadcast.events).mockResolvedValue({items:[]});vi.mocked(workspace.list).mockResolvedValue({items:[]})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><BroadcastTasks school="school" admin={admin}/></QueryClientProvider>)
}
it('does not equate member execution or absent results with group success',()=>{
 expect(targetLabel(target)).toContain('等待成员执行')
 expect(targetLabel({...target,official_status:1})).toContain('非已读')
 expect(targetLabel({...target,stop_state:'stopped'})).toContain('未取得')
})
it('double click claims once; retry after a lost response retains its idempotency key',async()=>{
 let reject:(e:Error)=>void=()=>{}
 vi.mocked(broadcast.action).mockImplementationOnce(()=>new Promise((_,r)=>{reject=r})).mockResolvedValue(task)
 show();fireEvent.click(await screen.findByRole('button',{name:'查看任务 123456'}))
 const button=await screen.findByRole('button',{name:'确认创建企业微信群发任务'})
 fireEvent.click(button);fireEvent.click(button)
 await waitFor(()=>expect(broadcast.action).toHaveBeenCalledTimes(1))
 reject(new Error('网络中断，请核对'))
 await screen.findByText('网络中断，请核对')
 fireEvent.click(button)
 await waitFor(()=>expect(broadcast.action).toHaveBeenCalledTimes(2))
 expect(vi.mocked(broadcast.action).mock.calls[0][3]).toBe(vi.mocked(broadcast.action).mock.calls[1][3])
 expect(vi.mocked(broadcast.action).mock.calls[0][4]).toEqual({revision:1})
})
it('previews direct text with no posts or templates, and saves only selected group and text',async()=>{
 show()
 vi.mocked(workspace.list).mockImplementation(async(_s,resource)=>{
  if(resource==='groups')return {items:[{id:'g',name:'受控测试群',active:true}]}
  throw new Error('没有社区内容或模板')
 })
 vi.mocked(broadcast.save).mockRejectedValue(new Error('保存失败，请重试'))
 fireEvent.click(await screen.findByRole('button',{name:'新建群发任务'}))
 const editor=await screen.findByRole('textbox',{name:'群发正文'})
 fireEvent.change(editor,{target:{value:'测试😀\n第二行'}})
 expect(screen.getByLabelText('正文预览').textContent).toBe('测试😀\n第二行')
 expect(workspace.list).not.toHaveBeenCalledWith(expect.anything(),'posts',expect.anything())
 expect(workspace.list).not.toHaveBeenCalledWith(expect.anything(),'templates')
 fireEvent.mouseDown(screen.getByRole('combobox',{name:'目标客户群'}))
 fireEvent.click(await screen.findByText('受控测试群'))
 fireEvent.click(screen.getByRole('button',{name:'保存并预览'}))
 await screen.findByText('保存失败，请重试')
 expect(broadcast.save).toHaveBeenCalledWith('school',expect.objectContaining({content_source:'text',text_content:'测试😀\n第二行',group_ids:['g']}),undefined)
 expect(vi.mocked(broadcast.save).mock.calls[0][1]).not.toHaveProperty('template_version_id')
 expect(vi.mocked(broadcast.save).mock.calls[0][1]).not.toHaveProperty('post_ids')
 expect((editor as HTMLTextAreaElement).value).toBe('测试😀\n第二行')
 expect(broadcast.action).not.toHaveBeenCalled()
})
it('operator can inspect text draft but cannot confirm it',async()=>{
 show(false,{...task,content_source:'text',template_version_id:null,post_ids:[]})
 fireEvent.click(await screen.findByRole('button',{name:'查看任务 123456'}))
 const button=await screen.findByRole('button',{name:'确认创建企业微信群发任务'})
 expect((button as HTMLButtonElement).disabled).toBe(true)
 fireEvent.click(button)
 expect(broadcast.action).not.toHaveBeenCalled()
 expect(screen.getByText('直接文本草稿需本校管理员检查并确认提交。')).toBeTruthy()
})
it('empty target directory still permits text preview but prevents saving',async()=>{
 show();fireEvent.click(await screen.findByRole('button',{name:'新建群发任务'}))
 fireEvent.change(await screen.findByRole('textbox',{name:'群发正文'}),{target:{value:'通知'}})
 await screen.findByText('暂无可用客户群。可先填写并预览正文，配置本校客户群后才能保存任务。')
 expect(screen.getByLabelText('正文预览').textContent).toBe('通知')
 expect((screen.getByRole('button',{name:'保存并预览'}) as HTMLButtonElement).disabled).toBe(true)
})

it('explains the actual deployment block next to the disabled submission action',async()=>{
 show(true,task,'企业微信群发开关未开启。')
 fireEvent.click(await screen.findByRole('button',{name:'查看任务 123456'}))
 const button=await screen.findByRole('button',{name:'确认创建企业微信群发任务'})
 expect((button as HTMLButtonElement).disabled).toBe(true)
 expect(screen.getAllByText('企业微信群发开关未开启。').length).toBe(2)
 expect(broadcast.action).not.toHaveBeenCalled()
})

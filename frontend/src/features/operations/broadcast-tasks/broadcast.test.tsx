import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import BroadcastTasks,{targetLabel} from './index'
import {broadcast,workspace,type BroadcastTask,type BroadcastTarget} from './api'
vi.mock('./api',()=>({broadcast:{list:vi.fn(),task:vi.fn(),capability:vi.fn(),targets:vi.fn(),events:vi.fn(),action:vi.fn()},workspace:{list:vi.fn()}}))
vi.mock('../../../shared/use-dialogs',()=>({useDialogs:()=>({confirm:async()=>true,prompt:async()=>'原因',contextHolder:null})}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
afterEach(()=>{cleanup();vi.clearAllMocks()})
const task:BroadcastTask={id:'task-123456',school_id:'school',creator_id:'actor',template_version_id:'v',content:'冻结通知',revision:1,state:'draft',stopped:false,created_at:'2026-10-22T00:00:00Z',total:2,sent:0,rejected:0,uncertain:0,pending:2,batch_count:1,post_ids:['p'],source_task_id:null,source_draft_id:null,resend_reason:''}
const target:BroadcastTarget={id:'target',group_id:'g',chat_id:'chat',group_name:'测试群',batch_id:'batch',sender:'owner',creation_state:'created',msgid:'official-task',rejected:false,official_status:null,send_time:null,member_status:2,last_synced_at:null,sync_error:null,error_code:null,stop_state:'none',reconciliation:null,operation_unknown:false}
function show(){
 vi.mocked(broadcast.list).mockResolvedValue({items:[task]});vi.mocked(broadcast.task).mockResolvedValue(task)
 vi.mocked(broadcast.capability).mockResolvedValue({id:'current',broadcast_available:true,management_available:true,reason:null})
 vi.mocked(broadcast.targets).mockResolvedValue({items:[target]});vi.mocked(broadcast.events).mockResolvedValue({items:[]});vi.mocked(workspace.list).mockResolvedValue({items:[]})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><BroadcastTasks school="school" admin/></QueryClientProvider>)
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

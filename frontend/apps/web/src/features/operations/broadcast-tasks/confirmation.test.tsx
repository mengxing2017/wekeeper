import {afterEach,expect,it,vi} from 'vitest'
import {configure,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import BroadcastTasks from './index'
import {broadcast,type BroadcastTask} from './api'
vi.mock('./api',()=>({broadcast:{list:vi.fn(),task:vi.fn(),capability:vi.fn(),targets:vi.fn(),events:vi.fn(),action:vi.fn()},workspace:{list:vi.fn()}}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
configure({asyncUtilTimeout:5000})
afterEach(()=>{cleanup();vi.clearAllMocks()})
it('actual confirmation dialog submits a draft and immediately displays server-confirmed state',async()=>{
 const task:BroadcastTask={id:'task-123456',school_id:'school',creator_id:'admin',content_source:'text',template_version_id:null,content:'测试通知',revision:1,state:'draft',stopped:false,created_at:'2026-10-22T00:00:00Z',total:1,sent:0,rejected:0,uncertain:0,pending:1,batch_count:1,post_ids:[],source_task_id:null,source_draft_id:null,resend_reason:''}
 vi.mocked(broadcast.list).mockResolvedValue({items:[task]})
 vi.mocked(broadcast.task).mockResolvedValue(task)
 vi.mocked(broadcast.targets).mockResolvedValue({items:[{id:'target',group_id:'g',chat_id:'chat',group_name:'测试群',batch_id:'batch',sender:'owner',creation_state:'queued',msgid:null,rejected:false,official_status:null,send_time:null,member_status:null,last_synced_at:null,sync_error:null,error_code:null,stop_state:'none',reconciliation:null,operation_unknown:false}]})
 vi.mocked(broadcast.events).mockResolvedValue({items:[]})
 vi.mocked(broadcast.capability).mockResolvedValue({id:'current',broadcast_available:true,management_available:true,reason:null})
 vi.mocked(broadcast.action).mockImplementation(async()=>{
  const confirmed={...task,state:'confirmed' as const}
  vi.mocked(broadcast.task).mockResolvedValue(confirmed)
  return confirmed
 })
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><BroadcastTasks school="school" admin/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'查看任务 123456'}))
 await screen.findByText('已选为目标群，等待确认提交')
 expect(screen.queryByText(/成员任务：/)).toBeNull()
 fireEvent.click(await screen.findByRole('button',{name:'确认创建企业微信群发任务'}))
 const dialog=(await screen.findByText('确认正文和全部 1 个客户群，创建企业微信任务？成员仍需确认发送。',{selector:'.ant-modal-title'})).closest<HTMLElement>('[role=dialog]')!
 expect(broadcast.action).not.toHaveBeenCalled()
 fireEvent.click(within(dialog).getByRole('button',{name:/^确\s*认$/}))
 await waitFor(()=>expect(broadcast.action).toHaveBeenCalledWith('school',task.id,'confirm',expect.any(String),{revision:1}))
 await screen.findByText(/已确认提交，后台正在创建/)
 expect(screen.queryByText(/草稿尚未提交到企业微信/)).toBeNull()
 expect(screen.queryByText(/成员任务：/)).toBeNull()
})

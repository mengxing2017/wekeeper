import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {cleanup,render,screen,waitFor,fireEvent} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {SourceAutomationTasks} from './automation'
const api=vi.hoisted(()=>({list:vi.fn(),deliveries:vi.fn()}))
vi.mock('./api',()=>({businessMessages:api,receiptReasonLabels:{route_no_targets:'没有目标群'}}))
const diagnostic={channel:'mobile',stage:'device_receipt',label:'设备接收',reason_code:'awaiting_receipt',explanation:'等待设备接收',severity:'normal',affected_targets:1,since:'2026-11-13T00:00:00Z',retry_at:null,deadline_at:null,owner:'device',next_action:'系统按现行规则自动换机',historical_risk:false}
const row={progress:{counts_complete:true,version:'v1',stage:'partial',total:2,diagnostics:[diagnostic],observed_at:'2026-11-13T00:00:38Z',projection_lag:false,results_as_of:null},milestones:{received_at:'2026-11-13T00:00:00Z',handoff_completed_at:null,delegated_at:'2026-11-13T00:00:01Z'},receipt_id:'receipt-1',id:'message-1',event_id:'event-1',state:'partial',content:'冻结原正文',reason_code:null,sending:{sent:1,pending:1,failed:0,unknown:0},created_at:'2026-11-13',delegates:[{id:'delegate-1',channel:'mobile',task_id:'task-1',target_count:2}]}
beforeEach(()=>{vi.clearAllMocks();window.history.replaceState(null,'','/admin?view=source-automation-tasks');api.list.mockResolvedValue({items:[row],has_more:false});api.deliveries.mockResolvedValue({items:[{id:'mobile:target-1',channel:'mobile',target_name:'测试群',task_id:'task-1',device_name:'当前手机',attempt_no:2,diagnostic}],has_more:false,version:'v1',observed_at:row.progress.observed_at})})
afterEach(cleanup)
function mount(){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SourceAutomationTasks/></QueryClientProvider>)}
it('shows frozen content and actual channel task results without old controls',async()=>{
 mount();await screen.findByText('event-1')
 expect(screen.getByText('成功 1 / 失败 0')).toBeTruthy()
 expect(screen.getByText('处理中 1 / 不确定 0')).toBeTruthy()
 fireEvent.click(screen.getByRole('button',{name:/Expand row|展开行/}))
 expect(await screen.findByText('冻结原正文')).toBeTruthy();expect(await screen.findByText('测试群')).toBeTruthy();expect(screen.getByText('task-1')).toBeTruthy();expect(screen.getByText('当前手机')).toBeTruthy();expect(screen.getByRole('link',{name:'查看任务与尝试'}).getAttribute('href')).toContain('task=task-1')
 expect(screen.queryByRole('button',{name:/停止任务|重检|推进可恢复|确认发送/})).toBeNull()
})
it('filters by explicit route without asking for a workspace',async()=>{
 window.history.replaceState(null,'','/admin?view=source-automation-tasks&route=route-1');mount()
 await waitFor(()=>expect(api.list).toHaveBeenCalledWith('route-1','',1,30,{}))
 expect(screen.getAllByRole('combobox')).toHaveLength(3)
 await screen.findByText('event-1');fireEvent.click(screen.getByRole('button',{name: /^刷\s*新$/}))
 await waitFor(()=>expect(api.list.mock.calls.length).toBeGreaterThan(1))
})
it('shows no-target failure and continues to display subsequent messages',async()=>{
 api.list.mockResolvedValue({items:[{...row,id:'empty',event_id:'no-target',state:'not_dispatched',reason_code:'route_no_targets',sending:null,delegates:[],progress:{...row.progress,total:0,diagnostics:[]}},row],has_more:false});mount()
 expect(await screen.findByText('没有目标群')).toBeTruthy();expect(screen.getByText('未下发成功')).toBeTruthy();expect(screen.getByText('event-1')).toBeTruthy()
})

it('shows live target progress and exposes projection lag instead of delegated',async()=>{
 api.list.mockResolvedValue({items:[{...row,state:'completed',sending:{sent:2,failed:0,pending:0,unknown:0},progress:{...row.progress,stage:'completed',projection_lag:true}}],has_more:false});mount()
 expect(await screen.findByText('全部成功')).toBeTruthy()
 expect(screen.queryByText('通道已受理')).toBeNull()
 expect(screen.getByText('持久汇总待更新；当前显示实时目标事实')).toBeTruthy()
})
it('marks detail version changes instead of mixing them with the summary',async()=>{
 api.deliveries.mockResolvedValue({items:[],has_more:false,version:'v2',observed_at:row.progress.observed_at});mount()
 await screen.findByText('event-1');fireEvent.click(screen.getByRole('button',{name:/Expand row|展开行/}))
 expect(await screen.findByText('目标进度已更新，当前明细与列表属于不同版本')).toBeTruthy()
})

import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {cleanup,render,screen,waitFor,fireEvent} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {SourceAutomationTasks} from './automation'
const api=vi.hoisted(()=>({list:vi.fn()}))
vi.mock('./api',()=>({businessMessages:api,receiptReasonLabels:{route_no_targets:'没有目标群'}}))
const row={id:'message-1',event_id:'event-1',state:'delegated',content:'冻结原正文',reason_code:null,sending:{sent:1,pending:1,failed:0,unknown:0},created_at:'2026-11-13',delegates:[{id:'delegate-1',channel:'mobile',task_id:'task-1',target_count:2}]}
beforeEach(()=>{vi.clearAllMocks();window.history.replaceState(null,'','/admin?view=source-automation-tasks');api.list.mockResolvedValue({items:[row],has_more:false})})
afterEach(cleanup)
function mount(){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SourceAutomationTasks/></QueryClientProvider>)}
it('shows frozen content and actual channel task results without old controls',async()=>{
 mount();await screen.findByText('event-1')
 expect(screen.getByText('成功 1 / 失败 0 / 待处理 1 / 不确定 0')).toBeTruthy()
 fireEvent.click(screen.getByRole('button',{name:/Expand row|展开行/}))
 expect(await screen.findByText('冻结原正文')).toBeTruthy();expect(screen.getByText('task-1')).toBeTruthy()
 expect(screen.queryByRole('button',{name:/停止任务|重检|推进可恢复|确认发送/})).toBeNull()
})
it('filters by explicit route without asking for a workspace',async()=>{
 window.history.replaceState(null,'','/admin?view=source-automation-tasks&route=route-1');mount()
 await waitFor(()=>expect(api.list).toHaveBeenCalledWith('route-1','',1))
 expect(screen.queryByRole('combobox')).toBeNull()
 await screen.findByText('event-1');fireEvent.click(screen.getByRole('button',{name: /^刷\s*新$/}))
 await waitFor(()=>expect(api.list.mock.calls.length).toBeGreaterThan(1))
})
it('shows no-target failure and continues to display subsequent messages',async()=>{
 api.list.mockResolvedValue({items:[{...row,id:'empty',event_id:'no-target',state:'failed',reason_code:'route_no_targets',sending:null,delegates:[]},row],has_more:false});mount()
 expect(await screen.findByText('没有目标群')).toBeTruthy();expect(screen.getByText('未下发成功')).toBeTruthy();expect(screen.getByText('event-1')).toBeTruthy()
})

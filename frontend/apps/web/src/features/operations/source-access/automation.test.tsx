import {beforeEach,expect,it,vi} from 'vitest'
import {render,screen,waitFor,fireEvent,configure} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {SourceAutomationTasks} from './automation'
configure({asyncUtilTimeout:10000})
const api=vi.hoisted(()=>({list:vi.fn(),detail:vi.fn(),targets:vi.fn(),events:vi.fn(),attempts:vi.fn(),stop:vi.fn(),retry:vi.fn()}))
vi.mock('./api',()=>({sourceAutomation:api,automationStateLabels:{running:'进行中',queued:'等待调度'}}))
const row={id:'receipt-1',decision_id:'decision-1',source_id:'source-1',route_id:'route-1',unit_id:'unit-1',channel:'mobile',workspace_id:'workspace-1',route_revision:1,event_id:'event-1',external_content_id:'post-1',state:'running',counts:{total:2,pending:1,sent:1,failed:0,unknown:0,stopped:0,cancelled:0},stopped:false,revision:1,created_at:'2026-11-10',has_unknown:false,can_stop:false,can_retry:false}
beforeEach(()=>{vi.clearAllMocks();window.history.replaceState(null,'','/admin?view=source-automation-tasks');api.list.mockResolvedValue({items:[row],has_more:false});api.detail.mockResolvedValue({task:row,frozen:{content:'冻结原正文',executed_by:'source-routing',authorized_by:'admin',reconstructed:false},composition:null});api.targets.mockResolvedValue({items:[{target_id:'target-1',name:'目标群',state:'queued'}],excluded:[]});api.events.mockResolvedValue({items:[],next_cursor:null});api.attempts.mockResolvedValue({items:[]})})
it('opens the actual read-only task detail and respects server control capabilities',async()=>{
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SourceAutomationTasks/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'任务详情'}))
 expect(await screen.findByText('冻结原正文')).toBeTruthy()
 expect(screen.getByRole('button',{name:'停止任务'})).toHaveProperty('disabled',true)
 expect(screen.getByRole('button',{name:'推进可恢复目标'})).toHaveProperty('disabled',true)
 expect(screen.queryByRole('button',{name:'确认发送'})).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'执行尝试'}))
 await waitFor(()=>expect(api.attempts).toHaveBeenCalledWith('receipt-1','target-1'))
})

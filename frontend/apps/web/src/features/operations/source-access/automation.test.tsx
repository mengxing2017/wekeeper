import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {cleanup,render,screen,waitFor,fireEvent,configure} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {SourceAutomationTasks} from './automation'
configure({asyncUtilTimeout:10000})
afterEach(cleanup)
const api=vi.hoisted(()=>({list:vi.fn(),detail:vi.fn(),targets:vi.fn(),events:vi.fn(),attempts:vi.fn(),observations:vi.fn(),stop:vi.fn(),retry:vi.fn()}))
vi.mock('./api',()=>({sourceAutomation:api,executionEvidenceLabels:{official_task_created:'官方任务已创建'},automationStateLabels:{running:'进行中',queued:'等待调度'}}))
const row={id:'receipt-1',decision_id:'decision-1',source_id:'source-1',route_id:'route-1',unit_id:'unit-1',channel:'mobile',workspace_id:'workspace-1',route_revision:1,event_id:'event-1',external_content_id:'post-1',state:'running',counts:{total:2,pending:1,sent:1,failed:0,unknown:0,stopped:0,cancelled:0},stopped:false,revision:1,created_at:'2026-11-10',has_unknown:false,can_stop:false,can_retry:false}
beforeEach(()=>{vi.clearAllMocks();window.history.replaceState(null,'','/admin?view=source-automation-tasks');api.list.mockResolvedValue({items:[row],has_more:false});api.detail.mockResolvedValue({task:row,frozen:{content:'冻结原正文',executed_by:'source-routing',authorized_by:'admin',reconstructed:false},composition:null,execution:{external_confirmation:'member_required'}});api.targets.mockResolvedValue({items:[{target_id:'target-1',name:'目标群',state:'queued'}],excluded:[]});api.events.mockResolvedValue({items:[],next_cursor:null});api.observations.mockResolvedValue({items:[{id:1,evidence_type:'official_task_created',observation_source:'wecom_api',observed_at:'2026-11-10T00:00:00Z',normalized_result:'pending'}],next_cursor:null});api.attempts.mockResolvedValue({items:[]})})
it('opens the actual read-only task detail and respects server control capabilities',async()=>{
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SourceAutomationTasks/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'任务详情'}))
 expect(await screen.findByText('冻结原正文')).toBeTruthy()
 expect(screen.getByText(/成员在微信侧确认，无需平台逐条人工确认/)).toBeTruthy()
 expect(screen.getByRole('button',{name:'停止任务'})).toHaveProperty('disabled',true)
 expect(screen.getByRole('button',{name:'推进可恢复目标'})).toHaveProperty('disabled',true)
 expect(screen.queryByRole('button',{name:'确认发送'})).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'执行尝试'}))
 await waitFor(()=>expect(api.attempts).toHaveBeenCalledWith('receipt-1','target-1'))
 fireEvent.click(screen.getByRole('tab',{name:'执行证据'}))
 expect(await screen.findByText('官方任务已创建')).toBeTruthy()
})

it('opens execution evidence directly from the task list',async()=>{
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SourceAutomationTasks/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'执行证据'}))
 expect((await screen.findByRole('tab',{name:'执行证据'})).getAttribute('aria-selected')).toBe('true')
 expect(await screen.findByText('官方任务已创建')).toBeTruthy()
 expect(api.observations).toHaveBeenCalledWith('receipt-1',0)
})
it('preserves channel and workspace from a business-page entry and opens linked evidence',async()=>{
 window.history.replaceState(null,'','/admin?view=source-automation-tasks&channel=wecom&workspace_id=workspace-1&task=receipt-1&section=evidence')
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SourceAutomationTasks/></QueryClientProvider>)
 await waitFor(()=>expect(api.list).toHaveBeenCalledWith(expect.objectContaining({channel:'wecom',workspace_id:'workspace-1'})))
 expect(await screen.findByText('官方任务已创建')).toBeTruthy()
 expect(screen.getByRole('tab',{name:'执行证据'}).getAttribute('aria-selected')).toBe('true')
 fireEvent.click(screen.getByRole('button',{name:'查看全部授权工作空间'}))
 await waitFor(()=>expect(api.list).toHaveBeenLastCalledWith(expect.objectContaining({channel:'wecom',workspace_id:''})))
})

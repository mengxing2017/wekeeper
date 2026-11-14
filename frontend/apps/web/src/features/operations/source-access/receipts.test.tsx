import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {SourceReceipts} from './receipts'
const api=vi.hoisted(()=>({list:vi.fn()}))
vi.mock('./api',()=>({sourceReceipts:api,receiptStageLabels:{not_dispatched:'未下发成功',task_created:'任务已创建'},receiptReasonLabels:{route_no_targets:'没有目标群'}}))
vi.mock('../advertising',()=>({AdCompositions:({receipt}:{receipt:string})=><div>广告决定 {receipt}</div>}))
const row={id:'receipt-1',event_id:'event-1',processing_stage:'not_dispatched',reason_code:'route_no_targets',received_at:'2026-11-13T00:00:00Z'}
function mount(){const q=new QueryClient({defaultOptions:{queries:{retry:false}}});render(<QueryClientProvider client={q}><SourceReceipts route="route-1"/></QueryClientProvider>)}
beforeEach(()=>{api.list.mockResolvedValue({items:[row],has_more:false})})
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('shows terminal non-delivery without a recheck or withdrawal workflow',async()=>{
 mount();await screen.findByText('event-1')
 expect(api.list).toHaveBeenCalledWith('route-1',1)
 expect(screen.getByText('未下发成功')).toBeTruthy()
 expect(screen.getByText('没有目标群')).toBeTruthy()
 expect(screen.queryByRole('button',{name:/重检|撤回|撤销/})).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'刷新收件'}))
 await waitFor(()=>expect(api.list.mock.calls.length).toBeGreaterThan(1))
})
it('paginates receipt evidence independently of current configuration',async()=>{
 api.list.mockResolvedValue({items:[row],has_more:true});mount();await screen.findByText('event-1')
 fireEvent.click(screen.getByRole('button',{name:/下一页/}))
 await waitFor(()=>expect(api.list).toHaveBeenCalledWith('route-1',2))
})

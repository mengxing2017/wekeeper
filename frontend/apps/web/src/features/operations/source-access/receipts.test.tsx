import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {SourceReceipts} from './receipts'
const keywords=vi.hoisted(()=>({checks:vi.fn().mockResolvedValue({items:[],has_more:false}),hits:vi.fn()}))
const api=vi.hoisted(()=>({list:vi.fn(),detail:vi.fn(),recheck:vi.fn()}))
vi.mock('./api',()=>({keywordRules:keywords,sourceReceipts:api,receiptStatusLabels:{rejected:'素材已拒绝',applied:'素材已应用'},receiptStageLabels:{rejected:'素材已拒绝',task_created:'任务已创建'},receiptReasonLabels:{keyword_blocked:'原文命中拦截规则'},routingIssueLabels:{}}))
const row={id:'receipt-1',event_id:'event-1',external_content_id:'notice-1',sequence:2,status:'rejected',processing_stage:'rejected',reason_code:'keyword_blocked',revision:7,observed_sequence:2,sending:null,received_at:'2026-11-09T00:00:00Z',updated_at:'2026-11-09T00:00:00Z',history:[],checks:[{rule_revision:1,fingerprint:'hash',outcome:'blocked',created_at:'2026-11-09T00:00:00Z'}],relations:[],can_recheck:true}
function mount(){const q=new QueryClient({defaultOptions:{queries:{retry:false}}});render(<QueryClientProvider client={q}><SourceReceipts route="route-1"/></QueryClientProvider>)}
beforeEach(()=>{keywords.checks.mockResolvedValue({items:[],has_more:false});api.list.mockResolvedValue({items:[row],has_more:false});api.detail.mockResolvedValue(row);api.recheck.mockResolvedValue({queued:true})})
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('keeps receipt identity and expected revision when requesting a reasoned recheck',async()=>{
 mount();await screen.findByText('event-1')
 expect(api.list).toHaveBeenCalledWith('route-1',1)
 fireEvent.click(screen.getByRole('button',{name:'查看回执'}))
 fireEvent.click(await screen.findByRole('button',{name:'按当前规则重检'}))
 fireEvent.click(screen.getByRole('button',{name:'提交重检'}))
 expect(await screen.findAllByText('请填写重检原因')).toHaveLength(2)
 expect(api.recheck).not.toHaveBeenCalled()
 fireEvent.change(screen.getByLabelText('重检原因'),{target:{value:'按当前词库复核'}})
 api.detail.mockResolvedValue({...row,status:'applied',processing_stage:'task_created',reason_code:null,revision:9,sending:{sent:0,pending:2,failed:0,unknown:0}})
 fireEvent.click(screen.getByRole('button',{name:'提交重检'}))
 await waitFor(()=>expect(api.recheck).toHaveBeenCalledWith('route-1','receipt-1',7,'按当前词库复核'))
 expect(await screen.findByText('任务已创建')).toBeTruthy()
 expect(api.recheck).toHaveBeenCalledTimes(1)
})
it('does not offer recheck to an operator without explicit authorization',async()=>{
 api.detail.mockResolvedValue({...row,can_recheck:false});mount()
 fireEvent.click(await screen.findByRole('button',{name:'查看回执'}))
 await screen.findByText('回执 ID')
 expect(screen.queryByRole('button',{name:'按当前规则重检'})).toBeNull()
 expect(api.recheck).not.toHaveBeenCalled()
})

import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {MobileTargetBindings} from './bindings'
import {sharedSend} from '../../admin/mobile-resources/api'
vi.mock('../../admin/mobile-resources/api',()=>({sharedSend:{usages:vi.fn(),usageOptions:vi.fn()}}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('使用管理保留真实现场阻塞，不把开通显示成检查通过',async()=>{
 vi.mocked(sharedSend.usageOptions).mockResolvedValue({id:'options',schools:[],devices:[],targets:[],scheme:null})
 vi.mocked(sharedSend.usages).mockResolvedValue({id:'usages',items:[{id:'usage',budget_root:'usage',supersedes:null,created_at:'2026-11-03T00:00:00Z',school_id:'school',school_name:'本校',revision:1,approval:'approved',state:'active',body:{device_ids:['phone'],targets:[{id:'target',name:'家长群',search_text:'家长群'}],max_calls:20,expires_at:'2099-01-01',auto_targets:false},expires_at:'2099-01-01',confirmed_until:'2099-01-01',delivery:'available',progress:'已开通，等待手机',events:[],issues:[],readiness:[{device_name:'共享手机',target_name:'家长群',state:'waiting_initial_consent',ready:false,messages:['等待手机首次启用托管']}]}]})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><MobileTargetBindings school="school" admin/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:/Expand row|展开/}))
 expect(await screen.findByText(/等待手机首次启用托管/)).toBeTruthy()
 expect(screen.queryByRole('button',{name:'重新评估自动检查'})).toBeNull()
 expect(screen.queryByText('可以用于发送任务')).toBeNull()
})

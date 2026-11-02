import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {MobileTargetBindings} from './bindings'
import {api,sharedSend,type SharedReadiness} from './api'
vi.mock('./api',()=>({api:{bindings:vi.fn(),targets:vi.fn()},sharedSend:{overview:vi.fn(),inspection:vi.fn()}}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('重新评估成功但尚不能排查时显示真实阻塞，不显示检查通过',async()=>{
 const state:SharedReadiness={binding_id:'binding',device_id:'phone',target_id:'target',device_name:'共享手机',target_name:'家长群',search_text:'家长群',state:'waiting_initial_consent',ready:false,automatic:true,blockers:[{code:'waiting_initial_consent',message:'等待手机首次启用托管',owner:'device',automatic:false}],inspection:null,inspection_state:'waiting_prerequisites',retry_at:null,retry_remaining:3}
 vi.mocked(api.bindings).mockResolvedValue({items:[{id:'binding',device_id:'phone',target_id:'target',school_id:'school',search_text_override:null,locator_revision:1,enabled:true,revision:1,device_name:'共享手机',search_text:'家长群'}]})
 vi.mocked(sharedSend.overview).mockResolvedValue({id:'school',tasks:[],drafts:[],readiness:[state],pool:[],inspections:[]})
 vi.mocked(sharedSend.inspection).mockResolvedValue({id:'binding',state:state.state,readiness:state})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><MobileTargetBindings school="school" admin/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'重新评估自动检查'}))
 expect(await screen.findByText('重新评估结果：家长群 / 共享手机')).toBeTruthy()
 expect(screen.getByText('等待手机首次启用托管')).toBeTruthy()
 expect(screen.queryByText('已重新评估检查需求，符合条件后自动执行，无需手机逐群确认')).toBeNull()
 expect(screen.queryByText('当前检查与发送条件已满足，请重新生成发送预览并核对。')).toBeNull()
})

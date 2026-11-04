import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {OfficialChannelControl} from './channel-control'
import {product} from './api'
vi.mock('./api',()=>({product:{officialChannelStatus:vi.fn(),officialChannelControl:vi.fn()}}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
afterEach(()=>{cleanup();vi.clearAllMocks()})
it('explicit pause requires a business confirmation even when automatic health has recovered',async()=>{
 vi.mocked(product.officialChannelStatus).mockResolvedValue({id:'current',configured:true,paused:true,revision:7,health_state:'paused',health:{checked_at:null,last_success_at:'2026-11-04T00:00:00Z',last_failure_at:null}})
 vi.mocked(product.officialChannelControl).mockRejectedValue(new Error('修订已变化，请刷新后重试'))
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><OfficialChannelControl/></QueryClientProvider>)
 await screen.findByText('已暂停新发送')
 expect(product.officialChannelControl).not.toHaveBeenCalled()
 fireEvent.click(screen.getByRole('button',{name:'恢复新发送'}))
 fireEvent.change(await screen.findByLabelText('操作原因'),{target:{value:'业务恢复'}})
 fireEvent.click(screen.getByRole('button',{name:'确认恢复'}))
 await waitFor(()=>expect(product.officialChannelControl).toHaveBeenCalledWith({paused:false,expected_revision:7,reason:'业务恢复'}))
 await screen.findByText('修订已变化，请刷新后重试')
 expect(screen.getByLabelText('操作原因')).toHaveProperty('value','业务恢复')
})

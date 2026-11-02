import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {BusinessSchemeEditor} from './business-scheme'
import {sharedSend} from './api'
vi.mock('./api',()=>({sharedSend:{saveBusinessScheme:vi.fn()}}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
function mount() {render(<QueryClientProvider client={new QueryClient()}><BusinessSchemeEditor status={{platform:'unconfigured',can_confirm:false,owner:'platform',message:'尚未配置'}}/></QueryClientProvider>)}
it('业务设置不依赖技术交付，保存仍需确认和理由',async()=>{
 vi.mocked(sharedSend.saveBusinessScheme).mockResolvedValue({id:'scheme',revision:1})
 mount();fireEvent.click(screen.getByRole('button',{name:'配置业务方案'}))
 expect(screen.getByRole('switch',{name:'开放新发送'}).hasAttribute('disabled')).toBe(false)
 expect((screen.getByLabelText('业务方案时区') as HTMLInputElement).value).toBe('Asia/Shanghai')
 expect((screen.getByRole('button',{name:'确认保存业务方案'}) as HTMLButtonElement).disabled).toBe(true)
 fireEvent.change(screen.getByLabelText('业务方案变更理由'),{target:{value:'首次业务配置'}})
 fireEvent.click(screen.getByRole('checkbox'));fireEvent.click(screen.getByRole('button',{name:'确认保存业务方案'}))
 await waitFor(()=>expect(sharedSend.saveBusinessScheme).toHaveBeenCalledOnce())
 const input=vi.mocked(sharedSend.saveBusinessScheme).mock.calls[0][0]
 expect(input).toMatchObject({enabled:false,expected_revision:0,confirmed:true,timezone:'Asia/Shanghai'})
 expect(input).not.toHaveProperty('allowed_stages');expect(input).not.toHaveProperty('release_id')
},20000)
it('可显式开放业务发送，冲突后保留表单并显示错误',async()=>{
 vi.mocked(sharedSend.saveBusinessScheme).mockRejectedValue(new Error('版本已变化，请刷新后核对'))
 mount();fireEvent.click(screen.getByRole('button',{name:'配置业务方案'}))
 fireEvent.click(screen.getByRole('switch',{name:'开放新发送'}))
 fireEvent.change(screen.getByLabelText('业务方案变更理由'),{target:{value:'确认开放'}})
 fireEvent.click(screen.getByRole('checkbox'));fireEvent.click(screen.getByRole('button',{name:'确认保存业务方案'}))
 expect(await screen.findByText('版本已变化，请刷新后核对')).toBeTruthy()
 expect(vi.mocked(sharedSend.saveBusinessScheme).mock.calls[0][0].enabled).toBe(true)
},20000)

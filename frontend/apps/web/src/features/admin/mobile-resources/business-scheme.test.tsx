import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {BusinessSchemeEditor,BusinessSchemePage} from './business-scheme'
import {sharedSend} from './api'
vi.mock('./api',()=>({sharedSend:{saveBusinessScheme:vi.fn(),businessScheme:vi.fn(),inheritBusinessScheme:vi.fn()}}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
function mount() {render(<QueryClientProvider client={new QueryClient()}><BusinessSchemeEditor/></QueryClientProvider>)}
it('业务设置不依赖技术交付，保存仍需确认和理由',async()=>{
 vi.mocked(sharedSend.saveBusinessScheme).mockResolvedValue({} as never)
 mount();fireEvent.click(screen.getByRole('button',{name:'配置业务方案'}))
 expect(screen.getByRole('switch',{name:'允许实际执行'}).hasAttribute('disabled')).toBe(false)
 expect((screen.getByLabelText('业务方案时区') as HTMLInputElement).value).toBe('Asia/Shanghai')
 expect((screen.getByRole('button',{name: '确认保存'}) as HTMLButtonElement).disabled).toBe(true)
 fireEvent.change(screen.getByLabelText('业务方案变更理由'),{target:{value:'首次业务配置'}})
 fireEvent.click(screen.getByRole('checkbox'));fireEvent.click(screen.getByRole('button',{name: '确认保存'}))
 await waitFor(()=>expect(sharedSend.saveBusinessScheme).toHaveBeenCalledOnce())
 const input=vi.mocked(sharedSend.saveBusinessScheme).mock.calls[0][0]
 expect(input).toMatchObject({enabled:true,expected_revision:0,confirmed:true,timezone:'Asia/Shanghai'})
 expect(input).not.toHaveProperty('allowed_stages');expect(input).not.toHaveProperty('release_id')
},20000)
it('可显式开放业务发送，冲突后保留表单并显示错误',async()=>{
 vi.mocked(sharedSend.saveBusinessScheme).mockRejectedValue(new Error('版本已变化，请刷新后核对'))
 mount();fireEvent.click(screen.getByRole('button',{name:'配置业务方案'}))
 fireEvent.change(screen.getByLabelText('业务方案变更理由'),{target:{value:'确认开放'}})
 fireEvent.click(screen.getByRole('checkbox'));fireEvent.click(screen.getByRole('button',{name: '确认保存'}))
 expect(await screen.findByText('版本已变化，请刷新后核对')).toBeTruthy()
 expect(vi.mocked(sharedSend.saveBusinessScheme).mock.calls[0][0].enabled).toBe(true)
},20000)

it('单位编辑使用本单位修订号并将保存限定在该单位',async()=>{
 const scheme={mode:'enabled',source:'default',revision:9,scope_revision:0,timezone:'Asia/Shanghai',allowed_windows:[{weekdays:[1,2,3,4,5,6,7],start_minute:0,end_minute:1440}],limits:{total:{interval_seconds:30,window_seconds:3600,max_calls:30,daily_max_calls:150},device:{interval_seconds:30,window_seconds:3600,max_calls:30,daily_max_calls:150},target:{interval_seconds:30,window_seconds:3600,max_calls:30,daily_max_calls:150}}} as const
 vi.mocked(sharedSend.businessScheme).mockResolvedValue(scheme as never)
 vi.mocked(sharedSend.saveBusinessScheme).mockResolvedValue(scheme as never)
 render(<QueryClientProvider client={new QueryClient()}><BusinessSchemePage unit="unit-a" unitName="单位甲"/></QueryClientProvider>)
 await screen.findByText(/当前来源：默认方案/)
 expect(sharedSend.businessScheme).toHaveBeenCalledWith('unit-a')
 expect(screen.queryByRole('button',{name:'恢复使用默认方案'})).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'配置业务方案'}))
 fireEvent.change(screen.getByLabelText('业务方案变更理由'),{target:{value:'单位独立配置'}})
 fireEvent.click(screen.getByRole('checkbox'));fireEvent.click(screen.getByRole('button',{name:'确认保存'}))
 await waitFor(()=>expect(sharedSend.saveBusinessScheme).toHaveBeenCalledOnce())
 expect(vi.mocked(sharedSend.saveBusinessScheme).mock.calls[0]).toEqual([expect.objectContaining({expected_revision:0}),expect.any(String),'unit-a'])
},20000)
it('恢复默认必须填写理由和确认并使用单位修订号',async()=>{
 vi.mocked(sharedSend.businessScheme).mockResolvedValue({source:'unit',scope_revision:3,mode:'disabled',timezone:'Asia/Shanghai',limits:{total:{daily_max_calls:10}}} as never)
 vi.mocked(sharedSend.inheritBusinessScheme).mockResolvedValue({} as never)
 render(<QueryClientProvider client={new QueryClient()}><BusinessSchemePage unit="unit-b"/></QueryClientProvider>)
 const button=await screen.findByRole('button',{name:'恢复使用默认方案'})
 expect((button as HTMLButtonElement).disabled).toBe(true)
 fireEvent.change(screen.getByLabelText('恢复默认理由'),{target:{value:'采用默认'}})
 fireEvent.click(screen.getByRole('checkbox'));fireEvent.click(button)
 await waitFor(()=>expect(sharedSend.inheritBusinessScheme).toHaveBeenCalledOnce())
 expect(vi.mocked(sharedSend.inheritBusinessScheme).mock.calls[0]).toEqual(['unit-b',{expected_revision:3,reason:'采用默认',confirmed:true},expect.any(String)])
},20000)

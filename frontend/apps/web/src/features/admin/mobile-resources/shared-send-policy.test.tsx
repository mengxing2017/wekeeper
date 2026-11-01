import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { SharedSendReadiness } from './shared-send-readiness'
import { sharedSend } from './api'
vi.mock('./api',()=>({sharedSend:{platform:vi.fn(),policy:vi.fn()}}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
async function open(policy: unknown = null) {
 vi.mocked(sharedSend.platform).mockResolvedValue({id:'current',schools:[],access:[],policy,releases:[],devices:[]} as Awaited<ReturnType<typeof sharedSend.platform>>)
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SharedSendReadiness/></QueryClientProvider>)
 await screen.findByText('当前政策：已禁用')
 await waitFor(()=>expect(sharedSend.platform).toHaveBeenCalled())
 fireEvent.click(screen.getByRole('button',{name:'配置发送政策'}))
}
it('新政策默认上海，新增时段可编辑中文星期和时分',async()=>{
 await open()
 expect((await screen.findByLabelText('IANA 时区') as HTMLInputElement).value).toBe('Asia/Shanghai')
 expect(document.querySelector('.ant-drawer-right')).toBeTruthy()
 expect(document.querySelector('.ant-modal')).toBeNull()
 expect(screen.getAllByLabelText('最小间隔（秒）').map(e=>(e as HTMLInputElement).value)).toEqual(['5','10','30','60'])
 expect(screen.getAllByLabelText('窗口最大调用数').map(e=>(e as HTMLInputElement).value)).toEqual(['120','60','30','10'])
 expect(screen.getAllByLabelText('每日最大调用数').map(e=>(e as HTMLInputElement).value)).toEqual(['1000','300','150','30'])
 fireEvent.click(screen.getByRole('button',{name:'添加允许时段'}))
 expect((await screen.findByLabelText('开始时间') as HTMLInputElement).value).toBe('09:00')
 expect((screen.getByLabelText('结束时间') as HTMLInputElement).value).toBe('18:00')
 fireEvent.mouseDown(screen.getByLabelText('执行星期'))
 expect(await screen.findByText('周日')).toBeTruthy()
 expect(screen.queryByText('星期7')).toBeNull()
 fireEvent.mouseDown(screen.getByLabelText('IANA 时区'))
 fireEvent.click(screen.getByRole('button',{name:'移除时段'}))
 fireEvent.click(screen.getByRole('button',{name:'保存政策'}))
 expect(await screen.findByText('请配置 1 至 32 个允许时段')).toBeTruthy()
 expect(sharedSend.policy).not.toHaveBeenCalled()
 fireEvent.click(screen.getByRole('button',{name:'添加允许时段'}))
 fireEvent.change(screen.getByLabelText('变更理由'),{target:{value:'启用初始限额'}})
 fireEvent.click(screen.getByRole('button',{name:'保存政策'}))
 await waitFor(()=>expect(sharedSend.policy).toHaveBeenCalled())
 expect(vi.mocked(sharedSend.policy).mock.calls[0][0].limits.device).toEqual({interval_seconds:30,window_seconds:3600,max_calls:30,daily_max_calls:150})
},60000)
it('既有时区保持且全天结束午夜提交1440',async()=>{
 const limit={interval_seconds:60,window_seconds:3600,max_calls:10,daily_max_calls:100}
 await open({id:'policy',revision:3,mode:'disabled',timezone:'UTC',allowed_windows:[{weekdays:[7],start_minute:0,end_minute:60}],limits:{global:limit,school:limit,device:limit,target:limit}})
 expect((await screen.findByLabelText('IANA 时区') as HTMLInputElement).value).toBe('UTC')
 fireEvent.change(screen.getByLabelText('变更理由'),{target:{value:'调整周日全天'}})
 fireEvent.change(screen.getByLabelText('开始时间'),{target:{value:'01:00'}})
 fireEvent.click(screen.getByRole('button',{name:'保存政策'}))
 expect(await screen.findByText('开始与结束不能相同；全天请选择 00:00 至 00:00')).toBeTruthy()
 expect(sharedSend.policy).not.toHaveBeenCalled()
 fireEvent.change(screen.getByLabelText('开始时间'),{target:{value:'00:00'}})
 fireEvent.change(screen.getByLabelText('结束时间'),{target:{value:'00:00'}})
 fireEvent.click(screen.getByRole('button',{name:'保存政策'}))
 await waitFor(()=>expect(sharedSend.policy).toHaveBeenCalled())
 const input=vi.mocked(sharedSend.policy).mock.calls[0][0]
 expect(input.timezone).toBe('UTC')
 expect(input.allowed_windows).toEqual([{weekdays:[7],start_minute:0,end_minute:1440}])
 expect(input.expected_revision).toBe(3)
 expect(input.limits).toEqual({global:limit,school:limit,device:limit,target:limit})
},60000)

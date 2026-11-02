import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { DeviceCapabilities } from './capabilities'
import { sharedSend, api } from './api'
vi.mock('./api',()=>({sharedSend:{platform:vi.fn()},api:{capabilityOpenings:vi.fn(),openCapability:vi.fn()}}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('只展示一次托管流程，不请求旧开通或现场邀请',async()=>{
 vi.mocked(sharedSend.platform).mockResolvedValue({flows:[],baselines:[],samples:[],acceptances:[],scopes:[],revocations:[],validations:[],support:[],withdrawals:[],diagnostics:[],artifacts:[],bindings:[],targets:[],activation:null,id:'current',schools:[],access:[],policy:null,releases:[],devices:[{device_id:'phone',name:'共享手机',enabled:true}]})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><DeviceCapabilities/></QueryClientProvider>)
 expect(await screen.findByText('通用能力办理')).toBeTruthy()
 expect(screen.getByRole('button',{name:'发起无发送验证'})).toBeTruthy()
 expect(screen.queryByRole('button',{name:'发送到手机确认'})).toBeNull()
 expect(screen.queryByRole('button',{name:'申请手机确认发送能力'})).toBeNull()
 expect(api.capabilityOpenings).not.toHaveBeenCalled()
 expect(api.openCapability).not.toHaveBeenCalled()
})

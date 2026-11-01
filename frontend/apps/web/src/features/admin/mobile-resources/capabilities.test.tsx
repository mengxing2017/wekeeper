import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { DeviceCapabilities } from './capabilities'
import { sharedSend, api } from './api'
vi.mock('./api',()=>({sharedSend:{platform:vi.fn()},api:{capabilityOpenings:vi.fn(),openCapability:vi.fn()}}))
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('只展示一次托管流程，不请求旧开通或现场邀请',async()=>{
 vi.mocked(sharedSend.platform).mockResolvedValue({id:'current',schools:[],access:[],policy:null,releases:[],devices:[{device_id:'phone',name:'共享手机',delegation_accepted:true,runtime:null,environment_snapshot_id:null,environment_fresh:false,matching_releases:[],risk_count:0,acceptance_count:0}]})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><DeviceCapabilities/></QueryClientProvider>)
 expect(await screen.findByText('本机已同意托管，绑定后自动检查')).toBeTruthy()
 expect(screen.queryByRole('button',{name:'发送到手机确认'})).toBeNull()
 expect(screen.queryByRole('button',{name:'申请手机确认发送能力'})).toBeNull()
 expect(api.capabilityOpenings).not.toHaveBeenCalled()
 expect(api.openCapability).not.toHaveBeenCalled()
})

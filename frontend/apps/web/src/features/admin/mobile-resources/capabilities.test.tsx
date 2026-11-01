import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { DeviceCapabilities } from './capabilities'
import { api, type CapabilityOpening } from './api'
vi.mock('./shared-send-readiness',()=>({SharedSendReadiness:()=>null}))
vi.mock('./api', () => ({ api: { platformFieldInvitations: vi.fn().mockResolvedValue({items:[]}), fieldChecks: vi.fn().mockResolvedValue({items:[]}), capabilityOpenings: vi.fn(), openCapability: vi.fn(), cancelOpening: vi.fn(), requestAdapterSupport: vi.fn() } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
function mount(row: CapabilityOpening) {
 vi.mocked(api.platformFieldInvitations).mockResolvedValue({items:[]});vi.mocked(api.fieldChecks).mockResolvedValue({items:[]})
 vi.mocked(api.capabilityOpenings).mockResolvedValue({ items: [row] })
 render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><DeviceCapabilities /></QueryClientProvider>)
}
const row: CapabilityOpening = {id:'phone',device_name:'共享手机 01',device_enabled:true,online:true,environment:null,observed_at:null,environment_fresh:false,configuration_revision:0,opening:null,release_count:0,matching_releases:[],release_available:false,consent_current:false,local_device_state:'unconfirmed',delivery_state:'onboarding_only',execution_ready:false,can_open:false,support_request:null,blockers:[{code:'product_not_provided',message:'当前产品版本尚未提供此业务能力，暂不能开通',owner:'产品研发',next_action:'提交适配支持请求'}]}
it('无适配发行时没有导入或假开通入口，支持请求提交到负责方', async () => {
 mount(row)
 fireEvent.click(await screen.findByRole('button',{name:'查看进度与下一步'}, {timeout:10000}))
 expect(await screen.findByText('产品尚未提供',{exact:true})).toBeTruthy()
 expect(screen.queryByRole('button',{name:'发送到手机确认'})).toBeNull()
 expect(document.querySelector('input[type="file"]')).toBeNull()
 expect(screen.queryByRole('switch')).toBeNull()
 vi.mocked(api.requestAdapterSupport).mockResolvedValue({id:'support',device_id:'phone',note:'支持',owner:'产品研发',revision:1,state:'submitted',response:null,created_at:'2026-11-01'})
 fireEvent.click(screen.getByRole('button',{name:'提交适配支持请求'}))
 await vi.waitFor(()=>expect(api.requestAdapterSupport).toHaveBeenCalledWith({device_id:'phone',note:'请适配此共享手机的企业微信群文字操作能力。'},expect.any(String)))
 expect(api.openCapability).not.toHaveBeenCalled()
})
it('刷新后显示已有申请和本机同意，禁止重复开通，保留取消与支持进度',async()=>{
 mount({...row,release_count:1,opening:{id:'application',device_id:'phone',capability_id:'cap',configuration_revision:1,revision:1,state:'awaiting_consent',release_id:'release',environment_snapshot_id:'env'},environment_fresh:true,release_available:true,consent_current:true,support_request:{id:'support',device_id:'phone',note:'支持',owner:'产品研发',revision:1,state:'investigating',response:null,created_at:'2026-11-01'}})
 fireEvent.click(await screen.findByRole('button',{name:'查看进度与下一步'}, {timeout:10000}))
 expect(screen.queryByRole('button',{name:'发送到手机确认'})).toBeNull()
 expect(screen.getByRole('button',{name:'取消开通申请'})).toBeTruthy()
 expect(await screen.findByText('适配支持请求：处理中')).toBeTruthy()
})

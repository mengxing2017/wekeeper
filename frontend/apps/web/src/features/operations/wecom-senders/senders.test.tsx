import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { OfficialSenders } from './index'
import { officialSenders } from './api'
vi.mock('./api',()=>({officialSenders:{list:vi.fn(),qualify:vi.fn()},distributionPolicies:{get:vi.fn().mockResolvedValue({sources:[],resources:[]}),source:vi.fn(),resource:vi.fn()}}))
vi.mock('../../../shared/use-dialogs',()=>({useDialogs:()=>({confirm:async()=>true,contextHolder:null})}))
vi.mock('./usage-manager',()=>({OfficialUsageManager:()=>null}))
vi.mock('./business-requests',()=>({BusinessRequests:({global}:{global?:boolean})=>global?<div>业务申请与问题处理</div>:null}))
vi.mock('./channel-control',()=>({OfficialChannelControl:()=> <div>官方通道控制</div>}))
vi.mock('./policies',()=>({DistributionPolicyPanel:()=> <div>分发策略</div>}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
afterEach(()=>{cleanup();vi.clearAllMocks()})
const sender={id:'account',userid:'verified-official',display_name:'官方成员',member_verified:true,member_active:true,account_active:true,binding_revision:2,qualification_revision:0,enabled:true,eligible:true,reason:'具体目标仍需核验'}
function show(){vi.mocked(officialSenders.list).mockResolvedValue({items:[sender]});render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><OfficialSenders/></QueryClientProvider>)}
it('global qualification uses official identity and optimistic version',async()=>{
 vi.mocked(officialSenders.qualify).mockResolvedValue({updated:true});show()
 fireEvent.click(await screen.findByRole('button',{name: '停用资格'}))
 await waitFor(()=>expect(officialSenders.qualify).toHaveBeenCalledWith('account',{enabled:false,revision:0}))
 expect(officialSenders.list).toHaveBeenCalledWith(1)
})
it('renders administrator controls without the Node.js global in browsers',async()=>{
 const nodeGlobal = globalThis.global
 vi.stubGlobal('global', undefined)
 try {
  show()
  expect(await screen.findByText('verified-official')).toBeTruthy()
  expect(screen.getByText('官方通道控制')).toBeTruthy()
  expect(screen.getByText('分发策略')).toBeTruthy()
  expect(screen.getByText('业务申请与问题处理')).toBeTruthy()
 } finally {
  vi.stubGlobal('global', nodeGlobal)
 }
})

import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { OfficialSenders } from './index'
import { officialSenders } from './api'
vi.mock('./api',()=>({officialSenders:{list:vi.fn(),qualify:vi.fn()},distributionPolicies:{get:vi.fn().mockResolvedValue({sources:[],resources:[]}),source:vi.fn(),resource:vi.fn()}}))
vi.mock('../../../shared/use-dialogs',()=>({useDialogs:()=>({confirm:async()=>true,contextHolder:null})}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
afterEach(()=>{cleanup();vi.clearAllMocks()})
const sender={id:'account',school_id:'school',userid:'verified-official',display_name:'官方成员',member_verified:true,member_active:true,account_active:true,binding_revision:2,qualification_revision:0,enabled:true,eligible:true,reason:'具体目标仍需核验'}
function show(global:boolean){vi.mocked(officialSenders.list).mockResolvedValue({items:[sender]});render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><OfficialSenders school="school" global={global}/></QueryClientProvider>)}
it('school summary is isolated and has no platform qualification or phone pairing actions',async()=>{
 show(false);await screen.findByText('verified-official');expect(officialSenders.list).toHaveBeenCalledWith('school',1)
 expect(screen.queryByRole('button',{name:'停用学校资格'})).toBeNull();expect(screen.queryByText('登记旧模式发送账号')).toBeNull();expect(screen.queryByText('手机配对')).toBeNull()
})
it('platform qualification uses school, official identity and optimistic version',async()=>{
 vi.mocked(officialSenders.qualify).mockResolvedValue({updated:true});show(true)
 fireEvent.click(await screen.findByRole('button',{name:'停用学校资格'}))
 await waitFor(()=>expect(officialSenders.qualify).toHaveBeenCalledWith('school','account',{enabled:false,revision:0}))
 expect(officialSenders.list).toHaveBeenCalledWith(undefined,1)
})

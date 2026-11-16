import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {ReceivingConfigurations} from './index'
import type {ReceivingConfiguration} from './api'
const mocks=vi.hoisted(()=>({list:vi.fn(),save:vi.fn(),resources:vi.fn()}))
vi.mock('./api',()=>({receivingConfigurations:()=>mocks,receivingUnits:vi.fn()}))
const config:ReceivingConfiguration={id:'configuration',business_unit_id:'unit',name:'接收一',enabled:true,target_ids:['target'],device_ids:['phone'],sender_ids:[]}
beforeEach(()=>{vi.clearAllMocks();mocks.list.mockResolvedValue({items:[config]});mocks.save.mockResolvedValue(config);mocks.resources.mockResolvedValue({targets:[],devices:[],senders:[]})})
afterEach(cleanup)
function show(admin=true){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false},mutations:{retry:false}}})}><ReceivingConfigurations channel="mobile" scope="unit" admin={admin} platformAdmin={admin}/></QueryClientProvider>)}
it('saves unit resources without workspace, version or exclusive ownership controls',async()=>{
 show();fireEvent.click(await screen.findByRole('button',{name:/编\s*辑/}));fireEvent.change(screen.getByLabelText('名称'),{target:{value:'新名称'}});fireEvent.click(screen.getByRole('button',{name:/保\s*存/}));
 await waitFor(()=>expect(mocks.save).toHaveBeenCalledWith('unit','configuration',{name:'新名称',enabled:true,target_ids:['target'],device_ids:['phone'],sender_ids:[]}));
 expect(screen.queryByText('版本')).toBeNull();expect(screen.queryByText('通道工作空间')).toBeNull()
})
it('unit viewers cannot edit configuration or select resources',async()=>{show(false);await screen.findByText('接收一');expect(screen.queryByRole('button',{name: /^新\s*增$/})).toBeNull();expect(mocks.resources).not.toHaveBeenCalled()})

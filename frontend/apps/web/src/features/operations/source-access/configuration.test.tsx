import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {RouteConfigurationEditor} from './configuration'
const api=vi.hoisted(()=>({configuration:vi.fn(),entities:vi.fn(),workspaces:vi.fn(),targets:vi.fn(),revisions:vi.fn(),configure:vi.fn(),lifecycle:vi.fn(),check:vi.fn(),handoff:vi.fn(),manager:vi.fn()}))
vi.mock('./api',()=>({sourceRouting:api,routingIssueLabels:{route_no_targets:'没有有效目标群'}}))
const initial={id:'route-1',revision:7,config_revision:2,unit_id:'unit-1',group_id:'group-1',status:'active',execution_state:'running',retired_at:null,managers:[],snapshot:{unit_name:'单位',group_name:'受众',execution:{channel:'mobile',workspace_id:'workspace-1',target_ids:['target-1']},targets:[{id:'target-1',name:'原目标群'}]}}
function mount(){const q=new QueryClient({defaultOptions:{queries:{retry:false}}});render(<QueryClientProvider client={q}><RouteConfigurationEditor route="route-1" onChanged={async()=>{}}/></QueryClientProvider>)}
beforeEach(()=>{
 api.configuration.mockResolvedValue(initial)
 api.entities.mockImplementation(async kind=>({items:kind==='routing-units'?[{id:'unit-1',name:'单位',active:true}]:[{id:'group-1',unit_id:'unit-1',name:'受众',active:true}]}))
 api.workspaces.mockResolvedValue({items:[{id:'workspace-1',name:'移动工作空间'}]})
 api.targets.mockResolvedValue({items:[{id:'target-1',name:'原目标群'},{id:'target-2',name:'第二群'}]})
 api.revisions.mockResolvedValue({items:[],has_more:false})
})
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('saves one complete revision and preserves local input when the server reports a conflict',async()=>{
 mount();await screen.findByText('运行中')
 await waitFor(()=>expect(api.targets).toHaveBeenCalledWith('mobile','workspace-1'))
 api.configure.mockRejectedValue(new Error('配置已更新'))
 const latest={...initial,revision:8,config_revision:3,snapshot:{...initial.snapshot,targets:[{id:'target-2',name:'他人新目标群'}]}}
 api.configuration.mockResolvedValue(latest)
 fireEvent.click(screen.getByRole('button',{name:'保存执行配置'}))
 await waitFor(()=>expect(api.configure).toHaveBeenCalledWith('route-1',{revision:7,unit_id:'unit-1',group_id:'group-1',execution:{channel:'mobile',workspace_id:'workspace-1',target_ids:['target-1']}}))
 expect(await screen.findByText('配置已被其他人修改，本地输入已保留')).toBeTruthy()
 expect(screen.getByText('他人新目标群')).toBeTruthy()
 expect((screen.getByRole('button',{name:'保存执行配置'}) as HTMLButtonElement).disabled).toBe(true)
 expect(api.configure).toHaveBeenCalledTimes(1)
})
it('pauses execution with a revision and reason without modifying caller authorization',async()=>{
 api.lifecycle.mockResolvedValue({revision:8});mount();await screen.findByText('运行中')
 fireEvent.click(screen.getByRole('button',{name:'暂停自动执行'}))
 fireEvent.change(await screen.findByLabelText('状态变更原因'),{target:{value:'暂停本周通知'}})
 fireEvent.click(screen.getByRole('button',{name:'确认变更'}))
 await waitFor(()=>expect(api.lifecycle).toHaveBeenCalledWith('route-1',7,'pause','暂停本周通知'))
})

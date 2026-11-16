import {cleanup,fireEvent,render,screen} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {RouteConfigurationEditor} from './configuration'
const api=vi.hoisted(()=>({configuration:vi.fn()}))
vi.mock('./api',()=>({sourceRouting:api}))
vi.mock('../receiving-configurations',()=>({ReceivingConfigurations:({channel,scope,admin}:{channel:string;scope:string;admin:boolean})=><div>{channel} {scope} {admin?'可编辑':'只读'}</div>}))
function mount(){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><RouteConfigurationEditor route="route-1"/></QueryClientProvider>)}
beforeEach(()=>{api.configuration.mockResolvedValue({id:'route-1',business_unit_id:'unit-1',name:'单位',status:'active'})})
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('connects both channel configurations to the business unit without revisions or workspaces',async()=>{
 mount();expect(await screen.findByText('业务单位：单位')).toBeTruthy()
 expect(screen.getByText('mobile unit-1 可编辑')).toBeTruthy();fireEvent.click(screen.getByRole('tab',{name:'企业微信'}));expect(await screen.findByText('wecom unit-1 可编辑')).toBeTruthy()
 expect(screen.queryByRole('button',{name:/发布版本|暂停自动执行|保存执行配置/})).toBeNull()
})
it('keeps administrator editing without a message-view authorization form',async()=>{
 mount();await screen.findByText('业务单位：单位')
 expect(screen.getByText('mobile unit-1 可编辑')).toBeTruthy()
 expect(screen.queryByRole('button',{name:'授权查看消息'})).toBeNull()
 expect(screen.queryByLabelText('路由配置管理员用户名')).toBeNull()
 expect(screen.queryByRole('table')).toBeNull()
})

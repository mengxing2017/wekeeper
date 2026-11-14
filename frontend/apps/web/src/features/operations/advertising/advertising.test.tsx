import {configure,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {Advertising,RouteAdvertising} from './index'
const api=vi.hoisted(()=>({list:vi.fn(),create:vi.fn(),publish:vi.fn(),placements:vi.fn(),bind:vi.fn(),setState:vi.fn(),scopes:vi.fn()}))
vi.mock('./api',()=>({scopedMaterials:{list:api.list,create:api.create,publish:api.publish},adPlacements:{list:api.placements,create:api.bind,setState:api.setState},advertisingScopes:api.scopes,advertising:{},advertisementReasonLabels:{}}))
configure({asyncUtilTimeout:5000})
beforeEach(()=>{
 api.list.mockResolvedValue({items:[{id:'m1',revision:1,archived:false,content:{name:'广告甲',body:'广告内容',link:null}}],has_more:false})
 api.placements.mockResolvedValue({items:[],has_more:false})
 api.scopes.mockResolvedValue({platforms:[{id:'p1',name:'平台甲'}],units:[{id:'u1',platform_id:'p1',name:'单位甲'}]})
 api.create.mockResolvedValue({id:'new'});api.bind.mockResolvedValue({id:'binding'})
})
afterEach(()=>{cleanup();vi.resetAllMocks()})
function show(node:React.ReactNode){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}>{node}</QueryClientProvider>)}
it('manages scoped ads without workspace controls and validates 140 Unicode characters',async()=>{
 show(<Advertising/>);await screen.findByText('广告甲');fireEvent.click(screen.getByRole('button',{name:'新增广告'}))
 fireEvent.change(await screen.findByLabelText('广告名称'),{target:{value:'新广告'}})
 fireEvent.change(screen.getByLabelText('广告正文'),{target:{value:'汉'.repeat(141)}})
 fireEvent.change(screen.getByLabelText('操作原因'),{target:{value:'业务投放'}})
 fireEvent.click(screen.getByRole('button',{name:'保存广告'}));await screen.findByText('单个广告正文最多 140 字');expect(api.create).not.toHaveBeenCalled()
 fireEvent.change(screen.getByLabelText('广告正文'),{target:{value:'😀'.repeat(140)}})
 fireEvent.click(screen.getByRole('button',{name:'保存广告'}));await waitFor(()=>expect(api.create).toHaveBeenCalledWith({content:{name:'新广告',body:'😀'.repeat(140),link:null},reason:'业务投放'}))
 expect(screen.queryByText('工作空间')).toBeNull()
})
it('does not offer obsolete route pools',()=>{
 show(<RouteAdvertising route="r1"/>);expect(screen.getByRole('link',{name:'管理广告投放'}).getAttribute('href')).toContain('view=advertisements')
 expect(screen.queryByLabelText('广告模式')).toBeNull()
})

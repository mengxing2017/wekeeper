import {configure} from '@testing-library/react'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {Advertising,RouteAdvertising} from './index'
const api=vi.hoisted(()=>({pool:vi.fn(),publishPool:vi.fn(),resetPool:vi.fn(),copyPool:vi.fn(),consumptions:vi.fn(),directory:vi.fn(),list:vi.fn(),detail:vi.fn(),versions:vi.fn(),create:vi.fn(),content:vi.fn(),archive:vi.fn(),policy:vi.fn(),config:vi.fn(),candidates:vi.fn(),configure:vi.fn(),preview:vi.fn()}))
vi.mock('./api',()=>({advertising:api,advertisementReasonLabels:{}}))
const config={mode:'fixed',fixed_ad_id:'ad1',insertion_position:'after',unavailable_policy:'without_ad',auto_recover:true}
const ad={id:'ad1',owner_unit_id:'u1',owner_name:'单位一',revision:2,content_version:'c1',policy_version:'p1',archived:false,content:{name:'共享素材',body:'保留原文的广告',link:null},policy:null,can_edit:false,can_manage_sharing:false,can_use:true}
// Async React Query renders need the same allowance as other form-heavy suites.
configure({asyncUtilTimeout:5000})
beforeEach(()=>{
 api.pool.mockResolvedValue({id:'r1',pool_id:null,revision:0,slots:[],cursor:null,transitions:[]})
 api.consumptions.mockResolvedValue({items:[],has_more:false})
 api.directory.mockResolvedValue({units:[],scopes:[],workspaces:[],routes:[],system_admin:false})
 api.list.mockResolvedValue({items:[ad],has_more:false});api.detail.mockResolvedValue(ad)
 api.config.mockResolvedValue({id:'r1',revision:4,config});api.candidates.mockResolvedValue({items:[ad],has_more:false});api.configure.mockResolvedValue({revision:5})
 api.preview.mockResolvedValue({content:'通知\n正文\nhttps://example.com\n\n【广告】\n共享素材',selection:{composer:'source-ad-text/1',exclusions:[],measurement:{bytes:80,max_bytes:8000,scalars:40,max_scalars:2000}},checks:{common:{total_hits:0},original:null,final:null},preview_only:true,deliverable:true})
})
afterEach(()=>{cleanup();vi.resetAllMocks()})
function show(node:React.ReactNode){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}>{node}</QueryClientProvider>)}
it('shared receiving administrator can read an advertisement without editing or reauthorizing it',async()=>{
 show(<Advertising/>);fireEvent.click(await screen.findByRole('button',{name:'查看广告'}))
 expect(await screen.findByText('保留原文的广告')).toBeTruthy()
 expect(screen.queryByRole('button',{name:'发布正文新版本'})).toBeNull();expect(screen.queryByRole('button',{name:'发布投放策略'})).toBeNull();expect(screen.queryByRole('button',{name:'新增广告'})).toBeNull()
 expect(screen.queryByRole('button',{name:'删除广告'})).toBeNull()
 expect(api.versions).not.toHaveBeenCalled()
})
it('route configuration explicitly saves the selected mode and previews actual text without sending',async()=>{
 show(<RouteAdvertising route="r1"/>);await screen.findByText('共享素材')
 fireEvent.change(screen.getByLabelText('广告配置原因'),{target:{value:'本路由主动选用'}})
 fireEvent.click(screen.getByRole('button',{name:'保存广告配置'}));await waitFor(()=>expect(api.configure).toHaveBeenCalledWith('r1',4,config,'本路由主动选用'))
 fireEvent.change(screen.getByLabelText('组合预览标题'),{target:{value:'通知'}});fireEvent.change(screen.getByLabelText('组合预览正文'),{target:{value:'正文'}});fireEvent.change(screen.getByLabelText('组合预览来源链接'),{target:{value:'https://example.com'}})
 fireEvent.click(screen.getByRole('button',{name:'预览组合（不创建任务）'}));await waitFor(()=>expect(api.preview).toHaveBeenCalledWith('r1',{title:'通知',content:'正文',detail_url:'https://example.com'}))
 expect(await screen.findByText(/来源消息检查 0/)).toBeTruthy();expect(screen.getByText(/【广告】/)).toBeTruthy();expect(api.create).not.toHaveBeenCalled()
})
it('a revision conflict retains the route draft and displays the server error',async()=>{
 api.configure.mockRejectedValue(new Error('记录已被修改，请核对最新修订'))
 show(<RouteAdvertising route="r1"/>);await screen.findByText('共享素材');fireEvent.change(screen.getByLabelText('广告配置原因'),{target:{value:'保留这份编辑'}});fireEvent.click(screen.getByRole('button',{name:'保存广告配置'}))
 expect(await screen.findByText('记录已被修改，请核对最新修订')).toBeTruthy();expect((screen.getByLabelText('广告配置原因') as HTMLTextAreaElement).value).toBe('保留这份编辑')
})

it('keeps the create entry visible for a system administrator without units and links to the directory',async()=>{
 api.directory.mockResolvedValue({units:[],scopes:[],workspaces:[],routes:[],system_admin:true})
 show(<Advertising/>);const create=await screen.findByRole('button',{name:'新增广告'})
 expect(create.hasAttribute('disabled')).toBe(true)
 expect(screen.getByText('请先创建或启用一个业务单位，再新增广告。')).toBeTruthy()
 expect(screen.getByRole('link',{name:'管理业务单位'}).getAttribute('href')).toBe('/admin?view=source-routing-directory')
 expect(screen.getByRole('link',{name:'配置广告自动序列轮询'}).getAttribute('href')).toBe('/admin?view=advertisement-rotations')
})
it('creates an advertisement from the page entry and opens the new detail',async()=>{
 api.directory.mockResolvedValue({units:[{id:'u1',name:'单位一',can_edit:true,can_share:false}],scopes:[],workspaces:[],routes:[],system_admin:false})
 api.create.mockResolvedValue({id:'new-ad',revision:2});api.detail.mockResolvedValue({...ad,id:'new-ad',can_edit:true,content:{name:'新广告',body:'新的正文',link:null}});api.versions.mockResolvedValue({items:[],has_more:false})
 show(<Advertising/>);fireEvent.click(await screen.findByRole('button',{name:'新增广告'}))
 fireEvent.change(await screen.findByLabelText('广告名称'),{target:{value:'新广告'}});fireEvent.change(screen.getByRole('textbox',{name:'广告正文'}),{target:{value:'新的正文'}});fireEvent.change(screen.getByLabelText('广告发布原因'),{target:{value:'新建素材'}})
 fireEvent.click(screen.getByRole('button',{name:'创建广告'}))
 await waitFor(()=>expect(api.create).toHaveBeenCalledWith({owner_unit_id:'u1',content:{name:'新广告',body:'新的正文',link:null},workspaces:[],reason:'新建素材'}))
 expect(await screen.findByRole('button',{name:'发布正文新版本'})).toBeTruthy()
 expect(api.detail).toHaveBeenCalledWith('new-ad')
})
it('deletes from the list with a reason and refreshes the active advertisements',async()=>{
 api.list.mockResolvedValue({items:[{...ad,can_edit:true}],has_more:false})
 api.archive.mockImplementation(async()=>{api.list.mockResolvedValue({items:[],has_more:false});return {id:'ad1',revision:3}})
 show(<Advertising/>);fireEvent.click(await screen.findByRole('button',{name:'删除广告'}))
 fireEvent.click(await screen.findByRole('button',{name:'确认删除'}))
 expect(await screen.findByText('请填写删除原因')).toBeTruthy();expect(api.archive).not.toHaveBeenCalled()
 fireEvent.change(screen.getByLabelText('删除广告原因'),{target:{value:'停止使用这条广告'}});fireEvent.click(screen.getByRole('button',{name:'确认删除'}))
 await waitFor(()=>expect(api.archive).toHaveBeenCalledWith('ad1',2,'停止使用这条广告'))
 await waitFor(()=>expect(screen.queryByRole('button',{name:'确认删除'})).toBeNull())
 await waitFor(()=>expect(screen.queryByText('共享素材')).toBeNull())
})

it('preserves independent duplicate slots, reorder identity and edit input after a pool conflict',async()=>{
 api.pool.mockResolvedValue({id:'r1',pool_id:'pool1',revision:2,slots:[{slot_id:'s1',ad_id:'ad1'},{slot_id:'s2',ad_id:'ad1'}],cursor:{pool_revision:2,cursor_version:4,reset_epoch:0,next_slot_id:'s2',last_consumed_slot_id:'s1',cycle:0,selection_order:1},transitions:[]})
 api.consumptions.mockResolvedValue({items:[],has_more:false,statistics:{selections:20,valid_targets:40,device_attempts:3,sent_targets:2,failed_targets:1,unknown_targets:1,pending_targets:36},slot_statistics:[{slot_id:'s1',ad_id:'ad1',selections:5},{slot_id:'s2',ad_id:'ad1',selections:5}],ad_statistics:[{ad_id:'ad1',selections:10}]})
 api.publishPool.mockRejectedValue(new Error('序列修订冲突'))
 show(<RouteAdvertising route="r1"/>);await screen.findByText('1 · s1')
 expect(await screen.findByText('广告累计选用（合并重复槽）')).toBeTruthy()
 expect(screen.getByText(/有效目标 40；设备尝试 3；实际送达 2；失败 1；不确定 1/)).toBeTruthy()
 fireEvent.click(screen.getAllByRole('button',{name:'下移'})[0])
 fireEvent.change(screen.getByLabelText('序列操作原因'),{target:{value:'移动重复广告的第二槽'}})
 fireEvent.click(screen.getByRole('button',{name:'发布序列'}))
 await waitFor(()=>expect(api.publishPool).toHaveBeenCalledWith('r1',{expected_revision:2,reason:'移动重复广告的第二槽',slots:[{slot_id:'s2',ad_id:'ad1'},{slot_id:'s1',ad_id:'ad1'}]}))
 expect(await screen.findByText('序列修订冲突')).toBeTruthy()
 expect((screen.getByLabelText('序列操作原因') as HTMLTextAreaElement).value).toBe('移动重复广告的第二槽')
 expect(api.resetPool).not.toHaveBeenCalled()
})

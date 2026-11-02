import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,configure,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {MemoryRouter} from 'react-router-dom'
import {TemplateManager} from './template-manager'
import {ResourceEditor} from './resource-editor'
import {product,workspace} from './api'
vi.mock('./api',()=>({workspace:{list:vi.fn()},product:{templates:{preview:vi.fn(),confirm:vi.fn()},resources:{artwork:vi.fn(),preview:vi.fn(),confirm:vi.fn(),upload:vi.fn()}}}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
configure({asyncUtilTimeout:5000})
afterEach(()=>{cleanup();vi.clearAllMocks()})
function show(element:React.ReactNode){vi.mocked(workspace.list).mockResolvedValue({items:[]});vi.mocked(product.resources.artwork).mockResolvedValue([]);return render(<MemoryRouter><QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}>{element}</QueryClientProvider></MemoryRouter>)}
it('publishes a new template only after the actual rendered preview and one confirmation',async()=>{
 vi.mocked(product.templates.preview).mockResolvedValue({id:'preview',digest:'digest',expires_at:'later',summary:{name:'通知',body:'{{school.name}}',revision:0,note:'',publish:true},rendered:'学校通知',sample:true})
 vi.mocked(product.templates.confirm).mockResolvedValue({id:'template',published:true,version_id:'version'})
 show(<TemplateManager school="school"/>);fireEvent.click(await screen.findByRole('button',{name:'新建模板'}))
 fireEvent.change(screen.getByLabelText('模板名称'),{target:{value:'通知'}});fireEvent.change(screen.getByLabelText('模板正文'),{target:{value:'{{school.name}}'}})
 fireEvent.click(screen.getByRole('button',{name:'预览并发布'}));await screen.findByText('学校通知')
 expect(product.templates.confirm).not.toHaveBeenCalled();expect(screen.queryByText('保存新版本')).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'确认发布模板'}))
 await waitFor(()=>expect(product.templates.confirm).toHaveBeenCalledWith('school',{preview_id:'preview',digest:'digest'}))
})
it('keeps a failed publication preview recoverable and never submits an unreviewed media URL',async()=>{
 const saved=vi.fn().mockResolvedValue(undefined)
 const item={id:'resource',revision:1,kind:'banner',title:'活动',content:'校园活动',image_ref:{kind:'asset',id:'art'},action:{kind:'none'},active:true,sort_order:0,starts_at:null,ends_at:null}
 vi.mocked(product.resources.preview).mockResolvedValue({id:'preview',digest:'digest',expires_at:'later',summary:item as never})
 vi.mocked(product.resources.confirm).mockRejectedValue(new Error('资源已变化，请重新核对'))
 show(<ResourceEditor school="school" item={item} onSaved={saved}/>);fireEvent.click(await screen.findByRole('button',{name:/^预\s*览$/}))
 await screen.findByText(/范围：当前学校/);expect(product.resources.confirm).not.toHaveBeenCalled();expect(screen.getByAltText('活动').getAttribute('src')).toContain('/resource-artwork/art')
 fireEvent.click(screen.getByRole('button',{name:'确认发布'}));await screen.findByText('资源已变化，请重新核对')
 expect(saved).not.toHaveBeenCalled();expect(screen.getByRole('button',{name:'返回编辑'})).toBeTruthy()
 expect(product.resources.preview).toHaveBeenCalledWith('school',expect.objectContaining({image_ref:{kind:'asset',id:'art'},action:{kind:'none'}}))
})

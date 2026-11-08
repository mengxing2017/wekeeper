import {afterEach,expect,it,vi} from 'vitest'
import {cleanup,configure,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {MemoryRouter} from 'react-router-dom'
import {TemplateManager} from './template-manager'
import {product,workspace} from './api'
vi.mock('./api',()=>({workspace:{list:vi.fn()},product:{templates:{preview:vi.fn(),confirm:vi.fn()}}}))
class Resize {observe(){}unobserve(){}disconnect(){}}
vi.stubGlobal('ResizeObserver',Resize)
Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:vi.fn(),removeListener:vi.fn(),addEventListener:vi.fn(),removeEventListener:vi.fn()})})
configure({asyncUtilTimeout:5000})
afterEach(()=>{cleanup();vi.clearAllMocks()})
function show(element:React.ReactNode){vi.mocked(workspace.list).mockResolvedValue({items:[]});return render(<MemoryRouter><QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}>{element}</QueryClientProvider></MemoryRouter>)}
it('publishes a new template only after the actual rendered preview and one confirmation',async()=>{
 vi.mocked(product.templates.preview).mockResolvedValue({id:'preview',digest:'digest',expires_at:'later',summary:{name:'通知',body:'{{school.name}}',revision:0,note:'',publish:true},rendered:'分发工作空间通知',sample:true})
 vi.mocked(product.templates.confirm).mockResolvedValue({id:'template',published:true,version_id:'version'})
 show(<TemplateManager school="school"/>);fireEvent.click(await screen.findByRole('button',{name:'新建模板'}))
 expect(workspace.list).toHaveBeenCalledWith('school','distribution-materials')
 fireEvent.change(screen.getByLabelText('模板名称'),{target:{value:'通知'}});fireEvent.change(screen.getByLabelText('模板正文'),{target:{value:'{{school.name}}'}})
 fireEvent.click(screen.getByRole('button',{name:'预览并发布'}));await screen.findByText('分发工作空间通知')
 expect(product.templates.confirm).not.toHaveBeenCalled();expect(screen.queryByText('保存新版本')).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'确认发布模板'}))
 await waitFor(()=>expect(product.templates.confirm).toHaveBeenCalledWith('school',{preview_id:'preview',digest:'digest'}))
})

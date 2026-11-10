import {cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {KeywordRules} from './keywords'
const api=vi.hoisted(()=>({list:vi.fn(),save:vi.fn()}))
vi.mock('./api',()=>({keywordRules:api}))
function show(){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><KeywordRules/></QueryClientProvider>)}
beforeEach(()=>{api.list.mockResolvedValue({revision:3,entries:[{word:'诈骗',enabled:true}]});api.save.mockResolvedValue({revision:4})})
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('直接显示敏感词并保存新增',async()=>{
 show();expect(await screen.findByText('诈骗')).toBeTruthy()
 expect(screen.queryByText('发布历史')).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'添加敏感词'}))
 fireEvent.change(screen.getByLabelText('敏感词'),{target:{value:'违法'}})
 fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/保\s*存/}))
 await waitFor(()=>expect(api.save).toHaveBeenCalledWith({expected_revision:3,entries:[{word:'诈骗',enabled:true},{word:'违法',enabled:true}]}))
})
it('编辑敏感词并保留并发版本',async()=>{
 show();await screen.findByText('诈骗')
 fireEvent.click(screen.getByRole('button',{name:/编\s*辑/}))
 fireEvent.change(screen.getByLabelText('敏感词'),{target:{value:'欺诈'}})
 fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/保\s*存/}))
 await waitFor(()=>expect(api.save).toHaveBeenCalledWith({expected_revision:3,entries:[{word:'欺诈',enabled:true}]}))
})
it('启停敏感词时保存当前列表',async()=>{
 show();await screen.findByText('诈骗')
 fireEvent.click(screen.getByRole('switch',{name:'诈骗启用状态'}))
 await waitFor(()=>expect(api.save).toHaveBeenCalledWith({expected_revision:3,entries:[{word:'诈骗',enabled:false}]}))
})
it('确认后删除敏感词',async()=>{
 show();await screen.findByText('诈骗')
 fireEvent.click(screen.getByRole('button',{name:/删\s*除/}))
 fireEvent.click(await screen.findByRole('button',{name:'确认删除'}))
 await waitFor(()=>expect(api.save).toHaveBeenCalledWith({expected_revision:3,entries:[]}))
})
it('并发冲突时保留输入',async()=>{
 api.save.mockRejectedValue(new Error('记录已被修改，请刷新后重试'))
 show();await screen.findByText('诈骗')
 fireEvent.click(screen.getByRole('button',{name:'添加敏感词'}))
 fireEvent.change(screen.getByLabelText('敏感词'),{target:{value:'新词'}})
 fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/保\s*存/}))
 expect(await within(screen.getByRole('dialog')).findByText('记录已被修改，请刷新后重试')).toBeTruthy()
 expect((screen.getByLabelText('敏感词') as HTMLInputElement).value).toBe('新词')
})

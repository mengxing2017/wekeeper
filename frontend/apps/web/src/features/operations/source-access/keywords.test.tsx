import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {QueryClient,QueryClientProvider} from '@tanstack/react-query'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {KeywordRules} from './keywords'
const api=vi.hoisted(()=>({scopes:vi.fn(),versions:vi.fn(),publish:vi.fn()}))
vi.mock('./api',()=>({keywordRules:api}))
beforeEach(()=>{
 api.scopes.mockResolvedValue({items:[{id:'mobile:w1',kind:'workspace',label:'运营',channel:'mobile',revision:0,can_edit:true,recheck_complete:true}]})
 api.versions.mockResolvedValue({items:[{id:'v0',revision:0,entries:[],reason:'initial empty',created_at:'2026-11-09'}],has_more:false})
 api.publish.mockResolvedValue({revision:1})
})
afterEach(()=>{cleanup();vi.resetAllMocks()})
it('publishes the selected authorized scope with revision, reason and enabled entries',async()=>{
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><KeywordRules/></QueryClientProvider>)
 fireEvent.click(await screen.findByRole('button',{name:'查看与发布'}))
 await waitFor(()=>expect(screen.getByRole('button',{name:'编辑当前规则'}).hasAttribute('disabled')).toBe(false))
 fireEvent.click(screen.getByRole('button',{name:'编辑当前规则'}))
 fireEvent.click(screen.getByRole('button',{name:'添加敏感词'}))
 fireEvent.change(screen.getByLabelText('敏感词 1'),{target:{value:'禁止'}})
 fireEvent.change(screen.getByLabelText('发布原因'),{target:{value:'补充工作空间限制'}})
 fireEvent.click(screen.getByRole('button',{name:'发布新版本'}))
 await waitFor(()=>expect(api.publish).toHaveBeenCalledWith('mobile:w1',{expected_revision:0,reason:'补充工作空间限制',entries:[{word:'禁止',enabled:true}]}))
})

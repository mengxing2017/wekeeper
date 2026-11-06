import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { UsageManagement } from './usage-management'
import { sharedSend } from './api'
vi.mock('./api',()=>({sharedSend:{usageOptions:vi.fn(),usages:vi.fn(),usagePreview:vi.fn(),usageConfirm:vi.fn(),usageAction:vi.fn(),usageIssue:vi.fn()}}))
afterEach(cleanup)
beforeEach(()=>{
 vi.resetAllMocks()
 vi.mocked(sharedSend.usageOptions).mockResolvedValue({id:'options',schools:[{id:'school',name:'测试学校'}],devices:[{id:'phone',name:'离线候选手机',revision:1,online:false}],targets:[],scheme:null})
 vi.mocked(sharedSend.usages).mockResolvedValue({id:'usages',items:[]})
 vi.mocked(sharedSend.usagePreview).mockImplementation(async(_s,input)=>({id:'summary',digest:'summary-digest',body:{...input,scheme:null}}))
 vi.mocked(sharedSend.usageConfirm).mockResolvedValue({id:'usages',items:[]})
})
function mount(school?:string,admin=true){render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><UsageManagement school={school} admin={admin}/></QueryClientProvider>)}
it('平台开通只有范围与摘要两步，离线手机及同页新增群无需研发表单',async()=>{
 mount()
 fireEvent.click(screen.getByRole('button',{name:'开通使用'}))
 fireEvent.mouseDown(screen.getByLabelText('使用学校'))
 fireEvent.click(await screen.findByText('测试学校'))
 fireEvent.mouseDown(screen.getByLabelText('候选手机'))
 fireEvent.click(await screen.findByText('离线候选手机（离线，恢复后执行）'))
 fireEvent.change(screen.getByLabelText('同页新增群'),{target:{value:'  通知群  '}})
 fireEvent.click(screen.getByRole('button',{name:'下一步：核对摘要'}))
 expect(await screen.findByText(/使用范围会保留，手机恢复可用后自动继续/)).toBeTruthy()
 const input=vi.mocked(sharedSend.usagePreview).mock.calls[0][1]
 expect(input.targets).toEqual([{id:null,name:'  通知群  ',search_text:'  通知群  '}]);expect(input.device_ids).toEqual(['phone'])
 expect(input).not.toHaveProperty('apk_id');expect(input).not.toHaveProperty('release_id')
 fireEvent.click(screen.getByRole('button',{name:'确认开通'}))
 await waitFor(()=>expect(sharedSend.usageConfirm).toHaveBeenCalledTimes(1))
 expect(vi.mocked(sharedSend.usageConfirm).mock.calls[0][1]).toEqual({preview_id:'summary',digest:'summary-digest'})
 expect(screen.queryByText('通用能力办理')).toBeNull()
},60000)
it('学校运营只查看本校使用进度，无开通或审批按钮',async()=>{
 mount('school',false)
 await waitFor(()=>expect(sharedSend.usages).toHaveBeenCalledWith('school'))
 expect(screen.queryByRole('button',{name:'开通使用'})).toBeNull()
 expect(screen.queryByRole('button',{name:'批准'})).toBeNull()
})
it('开通摘要展示手机专用搜索名，主列表折叠历史且待审批重新申请保留来源',async()=>{
 const base={id:'old',budget_root:'family',supersedes:null,created_at:'2026-11-01T00:00:00Z',school_id:'school',school_name:'测试学校',revision:1,approval:'approved' as const,state:'active' as const,body:{device_ids:['phone'],targets:[{id:'group',name:'通知群',search_text:'默认群'}],max_calls:100,expires_at:'2099-01-01T00:00:00Z',auto_targets:false},expires_at:'2099-01-01T00:00:00Z',confirmed_until:'2099-01-01T00:00:00Z',delivery:'available' as const,progress:'已关联',events:[],issues:[]}
 const next={...base,id:'next',supersedes:'old',created_at:'2026-11-02T00:00:00Z',approval:'pending' as const,revision:2}
 vi.mocked(sharedSend.usages).mockResolvedValue({id:'usages',items:[base,next]})
 vi.mocked(sharedSend.usageOptions).mockResolvedValue({id:'options',schools:[{id:'school',name:'测试学校'}],devices:[{id:'phone',name:'测试手机',revision:1,online:false}],targets:[{id:'group',name:'通知群',search_text:'默认群',revision:1}],scheme:null})
 vi.mocked(sharedSend.usagePreview).mockImplementation(async(_s,input)=>({id:'summary',digest:'digest',body:{...input,scheme:null,locators:[{device_id:'phone',device_name:'测试手机',target_id:'group',target_index:0,target_name:'通知群',search_text:'手机专用群',search_source:'override',binding_locator_revision:2,target_locator_revision:null}]}}))
 mount('school')
 fireEvent.click(await screen.findByRole('button',{name:'修改并重新申请'}))
 expect(screen.queryByRole('button',{name:'续期'})).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'下一步：核对摘要'}))
 expect(await screen.findByText('测试手机 / 通知群：实际搜索“手机专用群”（手机专用）')).toBeTruthy()
 expect(vi.mocked(sharedSend.usagePreview).mock.calls[0][1]).toMatchObject({renewal_of:'next',expected_revision:2})
},60000)

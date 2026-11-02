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

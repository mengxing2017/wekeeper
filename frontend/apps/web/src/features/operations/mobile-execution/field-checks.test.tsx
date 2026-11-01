import { afterEach, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { SchoolFieldChecks } from './field-checks'
import { Modal } from 'antd'
import { api, type TargetBinding } from './api'
vi.mock('./api',()=>({api:{fieldInvitations:vi.fn(),schoolFieldChecks:vi.fn(),inviteFieldCheck:vi.fn(),revokeFieldInvitation:vi.fn()}}))
afterEach(()=>{cleanup();vi.restoreAllMocks();vi.resetAllMocks()})
const binding:TargetBinding={id:'binding',school_id:'school',device_id:'phone',target_id:'target',search_text_override:null,locator_revision:1,target_locator_revision:3,enabled:true,revision:2,available:true,search_text:'手机备注  群',target_alias:'测试群',device_name:'共享手机'}
function mount(admin:boolean) {
 vi.mocked(api.fieldInvitations).mockResolvedValue({items:[]})
 vi.mocked(api.schoolFieldChecks).mockResolvedValue({items:[]})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SchoolFieldChecks school="school" admin={admin} bindings={[binding]} /></QueryClientProvider>)
}
it('学校操作员只能查看记录，不能发邀请',async()=>{
 mount(false)
 expect(await screen.findByText('当前可查看本校记录；请由本校管理员发出或撤销邀请。')).toBeTruthy()
 expect(screen.queryByRole('button',{name:/邀请检查：/})).toBeNull()
 expect(api.inviteFieldCheck).not.toHaveBeenCalled()
})
it('学校邀请固定绑定与目标版本，不提交管理员自填搜索文字',async()=>{
 const confirm=vi.spyOn(Modal,'confirm').mockReturnValue({destroy:vi.fn(),update:vi.fn()})
 mount(true)
 vi.mocked(api.inviteFieldCheck).mockResolvedValue({id:'invite',school_id:'school',device_id:'phone',target_id:'target',binding_id:'binding',binding_revision:2,target_locator_revision:3,search_text:'手机备注  群',revision:1,revoked_at:null,expires_at:'2026-11-02'})
 fireEvent.click(await screen.findByRole('button',{name:/邀请检查：/}))
 await waitFor(()=>expect(confirm).toHaveBeenCalledTimes(1))
 await act(async()=>{await confirm.mock.calls[0][0].onOk?.()})
 await waitFor(()=>expect(api.inviteFieldCheck).toHaveBeenCalledWith('school',{binding_id:'binding',expected_binding_revision:2,expected_target_locator_revision:3},expect.any(String)))
 expect(api.revokeFieldInvitation).not.toHaveBeenCalled()
})

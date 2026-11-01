import { afterEach, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { PlatformFieldChecks } from './field-checks'
import { Modal } from 'antd'
import { api, type CapabilityOpening } from './api'
vi.mock('./api',()=>({api:{platformFieldInvitations:vi.fn(),fieldChecks:vi.fn(),requestFieldCheck:vi.fn()}}))
afterEach(()=>{cleanup();vi.restoreAllMocks();vi.resetAllMocks()})
const row={id:'phone',consent_current:true,opening:{id:'opening'}} as CapabilityOpening
it('平台只能引用有效学校邀请，响应不明时重试同一负载和键',async()=>{
 const confirm=vi.spyOn(Modal,'confirm').mockReturnValue({destroy:vi.fn(),update:vi.fn()})
 vi.mocked(api.platformFieldInvitations).mockResolvedValue({items:[{id:'invite',school_id:'school',school_name:'学校甲',device_id:'phone',target_id:'target',binding_id:'binding',binding_revision:2,target_locator_revision:3,search_text:'现场备注群',revision:4,revoked_at:null,expires_at:'2026-11-02',available:true}]})
 vi.mocked(api.fieldChecks).mockResolvedValue({items:[]})
 vi.mocked(api.requestFieldCheck).mockRejectedValue(new Error('网络响应不明'))
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false},mutations:{retry:false}}})}><PlatformFieldChecks row={row} /></QueryClientProvider>)
 await waitFor(()=>expect(api.platformFieldInvitations).toHaveBeenCalledWith(1,'phone'))
 fireEvent.mouseDown(screen.getByRole('combobox'))
 fireEvent.click(await screen.findByText('学校甲 · 现场备注群'))
 fireEvent.click(screen.getByRole('button',{name:'发起无发送检查'}))
 await waitFor(()=>expect(confirm).toHaveBeenCalledTimes(1))
 await act(async()=>{await confirm.mock.calls[0][0].onOk?.()})
 await waitFor(()=>expect(api.requestFieldCheck).toHaveBeenCalledTimes(1))
 await screen.findByText('网络响应不明')
 fireEvent.click(screen.getByRole('button',{name:'重试同一检查申请'}))
 await waitFor(()=>expect(api.requestFieldCheck).toHaveBeenCalledTimes(2))
 expect(vi.mocked(api.requestFieldCheck).mock.calls[0]).toEqual(vi.mocked(api.requestFieldCheck).mock.calls[1])
 expect(vi.mocked(api.requestFieldCheck).mock.calls[0][0]).toEqual({opening_id:'opening',invitation_id:'invite',expected_invitation_revision:4})
})

import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ConfigurationWorkbench } from './configuration'
import { ExecutionDevices } from './index'
import { api } from './api'
vi.mock('../../admin/mobile-resources/capabilities', () => ({DeviceCapabilities:()=> <p>当前共享手机开通页面</p>}))
vi.mock('./api', () => ({api:{devices:vi.fn(),accounts:vi.fn(),targets:vi.fn(),dispatch:{flows:vi.fn().mockResolvedValue({items:[]}),releases:vi.fn().mockResolvedValue({items:[]}),validate:vi.fn(),publish:vi.fn(),pacing:vi.fn().mockResolvedValue({items:[]})}}}))
afterEach(()=>{cleanup();vi.clearAllMocks()})
it('旧高级适配入口不挂载写表单或旧查询，历史按需只读',async()=>{
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><ConfigurationWorkbench /></QueryClientProvider>)
 expect(screen.getByText('当前共享手机开通页面')).toBeTruthy()
 expect(api.targets).not.toHaveBeenCalled();expect(api.dispatch.flows).not.toHaveBeenCalled();expect(api.dispatch.releases).not.toHaveBeenCalled()
 expect(screen.queryByText('发起真实 v5 维护验证（无需安装快照）')).toBeNull()
 fireEvent.click(screen.getByRole('button',{name:'查看历史 v5 资料（只读）'}))
 await vi.waitFor(()=>expect(api.dispatch.flows).toHaveBeenCalledWith(1))
 expect(screen.getByText('旧 v5 已停用，不作为当前发送许可')).toBeTruthy()
 expect(api.dispatch.validate).not.toHaveBeenCalled();expect(api.dispatch.publish).not.toHaveBeenCalled();expect(api.dispatch.pacing).toHaveBeenCalledWith(1)
 expect(screen.queryByRole('textbox')).toBeNull()
})

it('rules 旧书签不会从父页面隐式加载旧发送账号或设备候选',()=>{
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><ExecutionDevices view="rules" /></QueryClientProvider>)
 expect(screen.getByText('当前共享手机开通页面')).toBeTruthy()
 expect(api.accounts).not.toHaveBeenCalled();expect(api.devices).not.toHaveBeenCalled();expect(api.targets).not.toHaveBeenCalled();expect(api.dispatch.flows).not.toHaveBeenCalled()
})

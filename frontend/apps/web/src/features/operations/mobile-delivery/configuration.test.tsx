import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ExecutionDevices } from './index'
import { api } from './api'
vi.mock('../../admin/mobile-resources/usage-management', () => ({UsageManagement:()=> <p>当前共享手机开通页面</p>}))
vi.mock('./api', () => ({api:{devices:vi.fn(),accounts:vi.fn(),targets:vi.fn(),dispatch:{flows:vi.fn().mockResolvedValue({items:[]}),releases:vi.fn().mockResolvedValue({items:[]}),validate:vi.fn(),publish:vi.fn(),pacing:vi.fn().mockResolvedValue({items:[]})}}}))
afterEach(()=>{cleanup();vi.clearAllMocks()})
it('设备页只提供当前设备和使用开通，不读取历史账号或 v5 资料',()=>{
 vi.mocked(api.devices).mockResolvedValue({items:[]})
 render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><ExecutionDevices /></QueryClientProvider>)
 expect(screen.getByRole('tab',{name:'手机设备'})).toBeTruthy()
 fireEvent.click(screen.getByRole('tab',{name:'业务使用开通'}))
 expect(screen.getByText('当前共享手机开通页面')).toBeTruthy()
 expect(screen.queryByRole('tab',{name:/历史/})).toBeNull()
 expect(api.accounts).not.toHaveBeenCalled();expect(api.targets).not.toHaveBeenCalled();expect(api.dispatch.flows).not.toHaveBeenCalled();expect(api.dispatch.releases).not.toHaveBeenCalled();expect(api.dispatch.pacing).not.toHaveBeenCalled()
})

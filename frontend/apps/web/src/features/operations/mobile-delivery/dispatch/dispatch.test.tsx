import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { DispatchWorkspace } from './index'
import { api } from '../api'
vi.mock('../api', () => ({ api: { targets:vi.fn(), groups:vi.fn(), dispatch:{assignments:vi.fn().mockResolvedValue({items:[]}),batches:vi.fn().mockResolvedValue({items:[]}),enabled:vi.fn(),preview:vi.fn(),confirm:vi.fn(),releases:vi.fn()} } }))
afterEach(() => {cleanup();vi.clearAllMocks()})
function show(view:'devices'|'compose') {render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><DispatchWorkspace school="school" view={view} openTask={vi.fn()} /></QueryClientProvider>)}
it('旧分配只读，不提供启用、发布或旧账号查询',async()=>{
 show('devices')
 await vi.waitFor(()=>expect(api.dispatch.assignments).toHaveBeenCalledWith('school',1))
 expect(screen.queryByRole('button',{name:'启用学校 v5 执行'})).toBeNull()
 expect(api.targets).not.toHaveBeenCalled();expect(api.dispatch.releases).not.toHaveBeenCalled();expect(api.dispatch.enabled).not.toHaveBeenCalled()
})
it('旧 compose 深链接只显示退出说明且不查询执行候选',()=>{
 show('compose')
 expect(screen.getByText('旧 v5 已停用，不作为当前发送许可')).toBeTruthy()
 expect(screen.queryByRole('textbox')).toBeNull()
 expect(api.dispatch.assignments).not.toHaveBeenCalled();expect(api.targets).not.toHaveBeenCalled();expect(api.dispatch.preview).not.toHaveBeenCalled();expect(api.dispatch.confirm).not.toHaveBeenCalled()
})

import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, configure, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { Workspace } from './index'
import { mobileWorkspaces } from '../../operations/mobile-execution/api'
import { wecomWorkspaces } from '../../operations/wecom-senders/api'
import { identity } from '../../identity/api'
vi.mock('../../identity/api', () => ({ identity: { session: vi.fn(), logout: vi.fn() }, subscribeUnauthorized: () => () => {} }))
vi.mock('../../operations/wecom-senders/api', () => ({ wecomWorkspaces: { list: vi.fn() } }))
vi.mock('../../operations/mobile-execution/api', () => ({ mobileWorkspaces: { list: vi.fn() } }))
vi.mock('../../operations/mobile-execution/tasks', () => ({ MobileExecutionTasks: ({school,admin}:{school:string;admin:boolean}) => <p>独立任务：{school} / {admin ? '可确认' : '只准备'}</p> }))
vi.mock('../../operations/mobile-execution/targets', () => ({ MobileTargets: ({school}:{school:string}) => <p>独立目标：{school}</p> }))
configure({ asyncUtilTimeout: 5000 })
afterEach(() => { cleanup(); vi.resetAllMocks() })
function show(view: string, platformAdmin: boolean, role?: 'admin' | 'operator') {
  vi.mocked(wecomWorkspaces.list).mockResolvedValue({items:[]})
  vi.mocked(identity.session).mockResolvedValue({ csrf: 'csrf', user: {id:'user',username:'mobile',display_name:'移动运营',platform_admin:platformAdmin,bound_school_id:null,grants:[],wecom_authenticated:false} })
  vi.mocked(mobileWorkspaces.list).mockResolvedValue({ items: role ? [{id:'standalone',name:'独立移动分发',role}] : [] })
  const cache = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={cache}><MemoryRouter initialEntries={[`/admin?view=${view}&school=retired-school`]}><Workspace/></MemoryRouter></QueryClientProvider>)
}
it('系统管理员无需学校即可进入手机任务并确认发送', async () => {
  show('mobile-execution-tasks', true, 'admin')
  expect(await screen.findByText('独立任务：standalone / 可确认')).toBeTruthy()
  expect(screen.queryByLabelText('当前学校')).toBeNull()
  expect(screen.queryByLabelText('企业微信分发工作空间')).toBeNull()
})
it('移动运营无学校授权仍能进入手机目标群', async () => {
  show('mobile-targets', false, 'operator')
  expect(await screen.findByText('独立目标：standalone')).toBeTruthy()
  expect(screen.queryByLabelText('当前学校')).toBeNull()
  expect(screen.queryByLabelText('企业微信分发工作空间')).toBeNull()
})
it('没有移动授权的账号不能通过直接地址进入目标群', async () => {
  show('mobile-targets', false)
  expect(await screen.findByText('当前账号没有此功能的访问权限')).toBeTruthy()
  expect(screen.queryByText('独立目标：standalone')).toBeNull()
})

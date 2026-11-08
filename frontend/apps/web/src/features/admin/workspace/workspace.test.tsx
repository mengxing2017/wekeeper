import { selectOption } from '../../../test/select-option'
import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, configure, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Workspace } from './index'
import { identity, workspace } from './api'
import { wecomWorkspaces } from '../../operations/wecom-senders/api'
import { mobileWorkspaces } from '../../operations/mobile-execution/api'
vi.mock('../../identity/api', async () => ({ identity: (await import('./api')).identity, subscribeUnauthorized: () => () => {} }))
vi.mock('./api', () => ({ identity: { session: vi.fn(), administrators: vi.fn(), resetAdministrator: vi.fn(), logout: vi.fn() }, workspace: { list: vi.fn(), mutate: vi.fn() } }))
vi.mock('../../operations/wecom-senders/api', () => ({ wecomWorkspaces: { list: vi.fn() } }))
vi.mock('../../operations/mobile-execution/api', () => ({ mobileWorkspaces: { list: vi.fn() } }))
class TestResizeObserver { observe() {} unobserve() {} disconnect() {} }
vi.stubGlobal('ResizeObserver', TestResizeObserver)
configure({ asyncUtilTimeout: 5000 })
afterEach(() => { cleanup(); vi.resetAllMocks() })
Object.defineProperty(window, 'matchMedia', { writable: true, value: (query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() }) })
function scopes() {
  vi.mocked(wecomWorkspaces.list).mockResolvedValue({items:[{id:'school',name:'独立企业微信',role:'admin'}]})
  vi.mocked(mobileWorkspaces.list).mockResolvedValue({items:[]})
}
function show(view: string) {
  scopes()
  vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'root',display_name:'Root',platform_admin:true,bound_school_id:null,grants:[],wecom_authenticated:false}})
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={[`/admin?view=${view}`]}><Workspace/></MemoryRouter></QueryClientProvider>)
}
it('keeps an existing group identity read-only while saving its revocation', async () => {
  vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'admin',display_name:'管理员',platform_admin:true,bound_school_id:null,grants:[],wecom_authenticated:false}})
  scopes()
  vi.mocked(workspace.list).mockResolvedValue({items:[{id:'g',name:'测试群',corp_id:'corp',chat_id:'chat',active:true,revision:1}], page:{number:1,size:30,has_more:false}})
  vi.mocked(workspace.mutate).mockResolvedValue({id:'g'})
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={['/admin?wecom_workspace=school&view=groups']}><Workspace/></MemoryRouter></QueryClientProvider>)
  // Wait for data with a cheap text query before computing button accessibility.
  const groupCard = (await screen.findByText('测试群')).closest<HTMLElement>('.ant-card')!
  fireEvent.click(within(groupCard).getByRole('button',{name:/编\s*辑/}))
  expect((screen.getByLabelText('企业 ID（与应用配置一致）') as HTMLInputElement).readOnly).toBe(true)
  expect((screen.getByLabelText('客户群 ID（保存后不可更换）') as HTMLInputElement).readOnly).toBe(true)
  await selectOption(screen.getByLabelText('启用状态'), '撤销')
  const groupEditor = screen.getByLabelText('启用状态').closest('form')!
  fireEvent.click(within(groupEditor).getByRole('button',{name:/^保\s*存$/}))
  await waitFor(()=>expect(workspace.mutate).toHaveBeenCalledWith('school','groups',expect.objectContaining({id:'g',corp_id:'corp',chat_id:'chat',active:false,revision:1}),'POST'))
})


it.each(['members','settings','schools','accounts','posts','comments','cases','resources','platform-resources'])('rejects retired management URL %s', async view => {
  show(view)
  expect(await screen.findByText('页面不存在')).toBeTruthy()
  expect(workspace.list).not.toHaveBeenCalled()
  for (const name of ['学校成员与权限','学校设置','学校管理','登录账号管理']) expect(screen.queryByRole('menuitem',{name})).toBeNull()
})
it('requires a fresh login after resetting the current platform administrator password', async () => {
  vi.mocked(identity.administrators).mockResolvedValue([{id:'u',username:'root',display_name:'平台维护者',active:true,platform_admin:true}])
  vi.mocked(identity.resetAdministrator).mockResolvedValue(undefined as never)
  show('administrators')
  fireEvent.click(within((await screen.findByText('平台维护者')).closest('tr')!).getByRole('button',{name:'重设密码'}))
  fireEvent.change(await screen.findByLabelText('新密码'),{target:{value:'changed-password'}})
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:'确认重设密码'}))
  await waitFor(()=>expect(identity.resetAdministrator).toHaveBeenCalledWith('u','changed-password'))
  expect(await screen.findByText('登录工作空间',{selector:'h1'})).toBeTruthy()
})

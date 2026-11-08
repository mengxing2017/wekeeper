import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { Workspace } from './index'
import { identity } from '../../identity/api'
import { workspace } from './api'
import { wecomWorkspaces } from '../../operations/wecom-senders/api'
import { mobileWorkspaces } from '../../operations/mobile-execution/api'
import { auditEvents } from '../audit/api'

vi.mock('../../identity/api', () => ({ identity: { session: vi.fn() }, subscribeUnauthorized: () => () => {} }))
vi.mock('./api', () => ({ workspace: { list: vi.fn() } }))
vi.mock('../../operations/wecom-senders', () => ({ OfficialSenders: ({school}:{school:string}) => <p>发送账号范围：{school}</p> }))
vi.mock('../../operations/broadcast-tasks', () => ({ default: ({school}:{school:string}) => <p>群发任务范围：{school}</p> }))
vi.mock('./template-manager', () => ({ TemplateManager: ({school}:{school:string}) => <p>消息模板范围：{school}</p> }))
vi.mock('../audit/api', () => ({ auditEvents: { list: vi.fn() } }))
vi.mock('../../operations/wecom-senders/api', () => ({ wecomWorkspaces: { list: vi.fn(), save: vi.fn(), members: vi.fn(), grant: vi.fn() } }))
vi.mock('../../operations/mobile-execution/api', () => ({ mobileWorkspaces: { list: vi.fn() } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
beforeEach(() => {
  vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'admin',username:'admin',display_name:'管理员',platform_admin:true,bound_school_id:null,grants:[],wecom_authenticated:false}})
  vi.mocked(wecomWorkspaces.list).mockResolvedValue({items:[{id:'one',name:'分发一',role:'admin'},{id:'two',name:'分发二',role:'admin'}]})
  vi.mocked(mobileWorkspaces.list).mockResolvedValue({items:[]})
  vi.mocked(workspace.list).mockResolvedValue({items:[]})
  vi.mocked(auditEvents.list).mockResolvedValue({items:[{id:'event',action:'identity.password.reset',actor_id:'admin',object_id:'user',request_id:'request',detail:{},created_at:'2026-11-08T00:00:00Z'}]})
  vi.mocked(wecomWorkspaces.members).mockResolvedValue({items:[]})
})
function show(view: string) {
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={[`/admin?view=${view}`]}><Workspace /></MemoryRouter></QueryClientProvider>)
}
it('system audit works without querying or selecting any distribution workspace', async () => {
  show('audit')
  expect(await screen.findByText('identity.password.reset', {}, {timeout:5000})).toBeTruthy()
  expect(screen.queryByLabelText('审计工作空间')).toBeNull()
  expect(screen.queryByLabelText('企业微信分发工作空间')).toBeNull()
  expect(screen.queryByRole('button',{name:'新建企业微信工作空间'})).toBeNull()
  expect(screen.queryByRole('button',{name:'企业微信分发权限'})).toBeNull()
  expect(wecomWorkspaces.list).not.toHaveBeenCalled()
  expect(mobileWorkspaces.list).not.toHaveBeenCalled()
  expect(workspace.list).not.toHaveBeenCalled()
  expect(auditEvents.list).toHaveBeenCalledWith(expect.stringContaining('page%5Bnumber%5D=1'),expect.any(AbortSignal))
})
it('workspace authorization does not grant system audit access', async () => {
  const session = await vi.mocked(identity.session)()
  vi.mocked(identity.session).mockResolvedValue({...session,user:{...session.user,platform_admin:false}})
  show('audit')
  expect(await screen.findByText('当前账号没有此功能的访问权限',{}, {timeout:5000})).toBeTruthy()
  expect(auditEvents.list).not.toHaveBeenCalled()
})
it('workspace management retains creation and grants to the selected workspace', async () => {
  vi.mocked(wecomWorkspaces.save).mockResolvedValue({id:'new',name:'新分发',role:'admin'})
  vi.mocked(wecomWorkspaces.grant).mockResolvedValue({id:'member',role:'operator'})
  show('wecom-workspaces')
  fireEvent.click(await screen.findByRole('button',{name:'新建企业微信工作空间'}, {timeout:5000}))
  fireEvent.change(screen.getByLabelText('企业微信工作空间名称'),{target:{value:'新分发'}})
  fireEvent.click(screen.getByRole('button',{name:'确认创建'}))
  await waitFor(() => expect(wecomWorkspaces.save).toHaveBeenCalledWith(expect.any(String),'新分发'))
  await waitFor(() => expect(screen.queryByRole('dialog',{name:'新建企业微信工作空间'})).toBeNull())
  fireEvent.click(within(screen.getByText('分发一').closest('tr')!).getByRole('button',{name:'分发权限'}))
  fireEvent.change(await screen.findByLabelText('授权登录账号'),{target:{value:'operator'}})
  fireEvent.click(screen.getByRole('button',{name:'保存分发授权'}))
  await waitFor(() => expect(wecomWorkspaces.grant).toHaveBeenCalledWith('one','operator','operator'))
  expect(workspace.list).not.toHaveBeenCalled()
})
it('accounts without distribution authorization cannot open workspace management', async () => {
  const session = await vi.mocked(identity.session)()
  vi.mocked(identity.session).mockResolvedValue({...session,user:{...session.user,platform_admin:false}})
  vi.mocked(wecomWorkspaces.list).mockResolvedValue({items:[]})
  show('wecom-workspaces')
  expect(await screen.findByText('当前账号没有此功能的访问权限',{}, {timeout:5000})).toBeTruthy()
  expect(screen.queryByRole('menuitem',{name:'工作空间与权限'})).toBeNull()
  expect(screen.queryByRole('button',{name:'新建企业微信工作空间'})).toBeNull()
})

it.each([
  ['groups', '查询结果'],
  ['templates', '消息模板范围：one'],
  ['broadcast-tasks', '群发任务范围：one'],
  ['wecom-senders', '发送账号范围：one'],
  ['wecom-workspaces', '当前工作空间'],
])('%s renders without the distribution workspace dropdown', async (view, content) => {
  show(view)
  expect(await screen.findByText(content, {}, {timeout:5000})).toBeTruthy()
  expect(screen.queryByLabelText('企业微信分发工作空间')).toBeNull()
  expect(screen.queryByPlaceholderText('选择企业微信分发工作空间')).toBeNull()
  if (view === 'groups') expect(workspace.list).toHaveBeenCalledWith('one','groups',expect.any(String),expect.any(AbortSignal))
})
it('changes the active workspace and manages permissions on the intended row', async () => {
  vi.mocked(wecomWorkspaces.grant).mockResolvedValue({id:'member',role:'operator'})
  show('wecom-workspaces')
  const second = (await screen.findByText('分发二', {}, {timeout:5000})).closest('tr')!
  fireEvent.click(within(second).getByRole('button',{name:'使用此工作空间'}))
  await waitFor(() => expect(within(second).getByText('当前工作空间')).toBeTruthy())
  fireEvent.click(within(second).getByRole('button',{name:'分发权限'}))
  fireEvent.change(await screen.findByLabelText('授权登录账号'),{target:{value:'operator'}})
  fireEvent.click(screen.getByRole('button',{name:'保存分发授权'}))
  await waitFor(() => expect(wecomWorkspaces.grant).toHaveBeenCalledWith('two','operator','operator'))
})

it('authorized operators can switch their workspaces without creation or permission controls', async () => {
  const session = await vi.mocked(identity.session)()
  vi.mocked(identity.session).mockResolvedValue({...session,user:{...session.user,platform_admin:false}})
  show('wecom-workspaces')
  const second = (await screen.findByText('分发二', {}, {timeout:5000})).closest('tr')!
  expect(screen.queryByRole('button',{name:'新建企业微信工作空间'})).toBeNull()
  expect(screen.queryByRole('button',{name:'分发权限'})).toBeNull()
  fireEvent.click(within(second).getByRole('button',{name:'使用此工作空间'}))
  await waitFor(() => expect(within(second).getByText('当前工作空间')).toBeTruthy())
  expect(wecomWorkspaces.grant).not.toHaveBeenCalled()
  expect(wecomWorkspaces.save).not.toHaveBeenCalled()
})

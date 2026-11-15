import {configure} from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { Workspace } from './index'
import { identity } from '../../identity/api'
import { workspace } from './api'
import { wecomWorkspaces } from '../../operations/wecom-senders/api'
import { mobileWorkspaces } from '../../operations/mobile-execution/api'
import { sourceRouting, sourceReceipts } from '../../operations/source-access/api'
import { auditEvents } from '../audit/api'

vi.mock('../../../app/shell', () => ({ AppShell: ({children}: {children: React.ReactNode}) => <>{children}</> }))
vi.mock('../../identity/api', () => ({ identity: { session: vi.fn() }, subscribeUnauthorized: () => () => {} }))
vi.mock('./api', () => ({ workspace: { list: vi.fn() } }))
vi.mock('../audit/api', () => ({ auditEvents: { list: vi.fn() } }))
vi.mock('../../operations/wecom-senders/api', () => ({ wecomWorkspaces: { list: vi.fn(), save: vi.fn(), members: vi.fn(), grant: vi.fn() } }))
vi.mock('../../operations/mobile-execution/api', () => ({ mobileWorkspaces: { list: vi.fn() } }))
vi.mock('../../operations/source-access/api', () => ({ sourceRouting: { entities: vi.fn(), assigned: vi.fn() }, sourceReceipts: {list:vi.fn()}, executionEvidenceLabels:{}, automationStateLabels:{}, receiptStatusLabels:{},receiptStageLabels:{},receiptReasonLabels:{},routingIssueLabels:{} }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
// Async React Query renders need the same allowance as other form-heavy suites.
configure({asyncUtilTimeout:5000})
beforeEach(() => {
  vi.mocked(sourceRouting.assigned).mockResolvedValue({items:[]})
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
  expect(screen.queryByRole('button',{name:'新建企业微信工作空间'})).toBeNull()
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


it('opens route configuration with explicit route authorization and no workspace role', async () => {
  const session = await vi.mocked(identity.session)()
  vi.mocked(identity.session).mockResolvedValue({...session,user:{...session.user,platform_admin:false}})
  vi.mocked(wecomWorkspaces.list).mockResolvedValue({items:[]})
  vi.mocked(sourceRouting.assigned).mockResolvedValue({items:[{id:'route-1',route_name:'授权路由',caller_name:'平台',business_unit_id:'unit',status:'active'}]})
  show('source-route-configurations')
  expect(await screen.findByText('业务单位接收配置', {selector: '.ant-card-head-title'})).toBeTruthy()
  await waitFor(() => expect(sourceRouting.assigned).toHaveBeenCalled())
})

it('opens receipts for the selected authorized route',async()=>{
 vi.mocked(sourceRouting.assigned).mockResolvedValue({items:[{id:'route-1',route_name:'通知路由',caller_name:'调用方一',business_unit_id:'unit',status:'active'}]})
 vi.mocked(sourceReceipts.list).mockResolvedValue({items:[],has_more:false})
 show('source-receipts')
 fireEvent.click(await screen.findByRole('button',{name:'查看收件与回执'}))
 await waitFor(()=>expect(sourceReceipts.list).toHaveBeenCalledWith('route-1',1))
 expect(await screen.findByRole('button',{name:'刷新收件'})).toBeTruthy()
 expect(screen.queryByRole('button',{name:'执行配置'})).toBeNull()
})

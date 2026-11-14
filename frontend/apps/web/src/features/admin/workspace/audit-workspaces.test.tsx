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
import { sourceRouting, sourceReceipts, businessMessages } from '../../operations/source-access/api'
import {scopedMaterials,adPlacements,advertisingScopes} from '../../operations/advertising/api'
import { auditEvents } from '../audit/api'

vi.mock('../../operations/advertising/api',()=>({scopedMaterials:{list:vi.fn()},adPlacements:{list:vi.fn()},advertisingScopes:vi.fn(),advertising:{},advertisementReasonLabels:{}}))
vi.mock('../../identity/api', () => ({ identity: { session: vi.fn() }, subscribeUnauthorized: () => () => {} }))
vi.mock('./api', () => ({ workspace: { list: vi.fn() } }))
vi.mock('../../operations/wecom-senders', () => ({ OfficialSenders: ({school}:{school:string}) => <p>发送账号范围：{school}</p> }))
vi.mock('../../operations/broadcast-tasks', () => ({ default: ({school}:{school:string}) => <p>群发任务范围：{school}</p> }))
vi.mock('./template-manager', () => ({ TemplateManager: ({school}:{school:string}) => <p>消息模板范围：{school}</p> }))
vi.mock('../audit/api', () => ({ auditEvents: { list: vi.fn() } }))
vi.mock('../../operations/wecom-senders/api', () => ({ wecomWorkspaces: { list: vi.fn(), save: vi.fn(), members: vi.fn(), grant: vi.fn() } }))
vi.mock('../../operations/mobile-execution/api', () => ({ mobileWorkspaces: { list: vi.fn() } }))
vi.mock('../../operations/source-access/api', () => ({ sourceRouting: { entities: vi.fn(), assigned: vi.fn() }, sourceReceipts: {list:vi.fn()}, businessMessages:{list:vi.fn()}, executionEvidenceLabels:{}, automationStateLabels:{}, receiptStatusLabels:{},receiptStageLabels:{},receiptReasonLabels:{},routingIssueLabels:{} }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
// Async React Query renders need the same allowance as other form-heavy suites.
configure({asyncUtilTimeout:5000})
beforeEach(() => {
  vi.mocked(scopedMaterials.list).mockResolvedValue({items:[],has_more:false})
  vi.mocked(adPlacements.list).mockResolvedValue({items:[],has_more:false})
  vi.mocked(advertisingScopes).mockResolvedValue({platforms:[],units:[]})
  vi.mocked(businessMessages.list).mockResolvedValue({items:[],has_more:false})
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


it.each([
  ['路由执行配置', '业务单位接收配置'],
  ['收件与回执', '收件与回执'],
])('opens the actual %s page from the sidebar', async (entry, content) => {
  show('audit')
  await screen.findByText('identity.password.reset')
  fireEvent.click(screen.getByRole('menuitem', {name: entry}))
  expect(await screen.findByText(content, {selector: '.ant-card-head-title'})).toBeTruthy()
  await waitFor(() => expect(sourceRouting.assigned).toHaveBeenCalled())
})
it('opens route configuration with explicit route authorization and no workspace role', async () => {
  const session = await vi.mocked(identity.session)()
  vi.mocked(identity.session).mockResolvedValue({...session,user:{...session.user,platform_admin:false}})
  vi.mocked(wecomWorkspaces.list).mockResolvedValue({items:[]})
  vi.mocked(sourceRouting.assigned).mockResolvedValue({items:[{id:'route-1',route_name:'授权路由',caller_name:'平台',business_unit_id:'unit',status:'active'}]})
  show('source-route-configurations')
  expect(await screen.findByText('业务单位接收配置', {selector: '.ant-card-head-title'})).toBeTruthy()
  await waitFor(() => expect(sourceRouting.assigned).toHaveBeenCalled())
  expect(screen.queryByRole('menuitem', {name:'调用方凭据'})).toBeNull()
  expect(screen.queryByRole('menuitem', {name:'业务单位与分组'})).toBeNull()
})

it('opens scoped receipts from the direct sidebar entry through the route row',async()=>{
 vi.mocked(sourceRouting.assigned).mockResolvedValue({items:[{id:'route-1',route_name:'通知路由',caller_name:'调用方一',business_unit_id:'unit',status:'active'}]})
 vi.mocked(sourceReceipts.list).mockResolvedValue({items:[],has_more:false})
 show('audit')
 await screen.findByText('identity.password.reset')
 fireEvent.click(screen.getByRole('menuitem',{name:'收件与回执'}))
 fireEvent.click(await screen.findByRole('button',{name:'查看收件与回执'}))
 await waitFor(()=>expect(sourceReceipts.list).toHaveBeenCalledWith('route-1',1))
 expect(await screen.findByRole('button',{name:'刷新收件'})).toBeTruthy()
 expect(screen.queryByRole('button',{name:'执行配置'})).toBeNull()
})

it('opens advertisement rotation as direct scope bindings without route pools',async()=>{
 show('audit')
 await screen.findByText('identity.password.reset')
 fireEvent.click(screen.getByRole('menuitem',{name:'广告序列轮询'}))
 expect(await screen.findByRole('button',{name:'绑定投放范围'})).toBeTruthy()
 expect(screen.queryByLabelText('广告模式')).toBeNull()
 expect(scopedMaterials.list).toHaveBeenCalledWith(1)
 expect(adPlacements.list).toHaveBeenCalledWith(1)
})

it('opens the actual advertising page without a workspace dependency',async()=>{
 vi.mocked(wecomWorkspaces.list).mockResolvedValue({items:[]})
 show('advertisements')
 expect(await screen.findByRole('button',{name:'新增广告'}, {timeout:15000})).toBeTruthy()
 expect(await screen.findByRole('menuitem',{name:'广告素材与投放'}, {timeout:15000})).toBeTruthy()
 expect(screen.queryByLabelText('企业微信分发工作空间')).toBeNull()
 expect(screen.queryByLabelText('移动分发工作空间')).toBeNull()
})

it('opens automatic execution results from the actual sidebar',async()=>{
 show('audit')
 await screen.findByText('identity.password.reset')
 fireEvent.click(screen.getByRole('menuitem',{name:'即时消息与发送结果'}))
 expect(await screen.findByRole('button',{name:'刷新消息'},{timeout:15000})).toBeTruthy()
 await waitFor(()=>expect(businessMessages.list).toHaveBeenCalled())
 expect(screen.queryByLabelText('执行通道')).toBeNull()
 expect(screen.queryByLabelText('移动分发工作空间')).toBeNull()
})

import { selectOption } from '../../../test/select-option'
import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, configure, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Workspace } from './index'
import { identity, workspace } from './api'
vi.mock('./api', () => ({ identity: { session: vi.fn(), accounts: vi.fn(), create: vi.fn(), update: vi.fn(), reset: vi.fn(), logout: vi.fn() }, workspace: { schools: vi.fn(), list: vi.fn(), createSchool: vi.fn(), updateSchool: vi.fn(), mutate: vi.fn() } }))
class TestResizeObserver { observe() {} unobserve() {} disconnect() {} }
vi.stubGlobal('ResizeObserver', TestResizeObserver)
configure({ asyncUtilTimeout: 5000 })
const originalComputedStyle = window.getComputedStyle;
window.getComputedStyle = element => originalComputedStyle(element);
afterEach(() => { cleanup(); vi.resetAllMocks() })
Object.defineProperty(window, 'matchMedia', { writable: true, value: (query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() }) })
it('restores the selected school and business page from its URL', async () => {
  vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'admin',display_name:'管理员',platform_admin:true,bound_school_id:null,grants:[],wecom_authenticated:false}})
  vi.mocked(workspace.schools).mockResolvedValue([{id:'school',name:'测试学校',active:true,revision:1,config:{}}])
  vi.mocked(workspace.list).mockResolvedValue({items:[], page:{number:1,size:30,has_more:false}})
  vi.mocked(identity.accounts).mockResolvedValue([])
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={['/admin?school=school&view=groups']}><Workspace/></MemoryRouter></QueryClientProvider>)
  expect(await screen.findByRole('heading', { name: '客户群', level: 1 })).toBeTruthy()
  await waitFor(() => expect(workspace.list).toHaveBeenCalledWith('school','groups',expect.any(String),expect.any(AbortSignal)))
  fireEvent.click(screen.getByRole('menuitem', { name: '学校管理' }))
  expect(await screen.findByRole('heading', { name: '学校管理', level: 1 })).toBeTruthy()
  expect(screen.queryByLabelText('当前学校')).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: '新建学校' }))
  expect(await screen.findByLabelText('学校名称')).toBeTruthy()
})

it('keeps an existing group identity read-only while saving its revocation', async () => {
  vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'admin',display_name:'管理员',platform_admin:true,bound_school_id:null,grants:[],wecom_authenticated:false}})
  vi.mocked(workspace.schools).mockResolvedValue([{id:'school',name:'测试学校',active:true,revision:1,config:{}}])
  vi.mocked(workspace.list).mockResolvedValue({items:[{id:'g',name:'测试群',corp_id:'corp',chat_id:'chat',active:true,revision:1}], page:{number:1,size:30,has_more:false}})
  vi.mocked(workspace.mutate).mockResolvedValue({id:'g'})
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={['/admin?school=school&view=groups']}><Workspace/></MemoryRouter></QueryClientProvider>)
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

function showManagement(view: string, platformAdmin = true, role: 'member' | 'operator' | 'admin' = 'admin') {
  vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'root',display_name:'Root',platform_admin:platformAdmin,bound_school_id:null,grants:[{school_id:'school',role}],wecom_authenticated:false}})
  vi.mocked(workspace.schools).mockResolvedValue([{id:'school',name:'甲学校',active:true,revision:1,config:{}}, {id:'other',name:'乙学校',active:true,revision:1,config:{}}])
  vi.mocked(workspace.list).mockResolvedValue({items:[]})
  vi.mocked(identity.accounts).mockResolvedValue([
    {id:'u',username:'root',display_name:'平台维护者',platform_admin:true,active:true},
    {id:'ordinary',username:'ordinary',display_name:'本地管理员',platform_admin:false,active:true,wecom_userid:'wx-user'},
  ])
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={[`/admin?school=school&view=${view}`]}><Workspace/></MemoryRouter></QueryClientProvider>)
}

it('separates ordinary accounts and administrators by privilege, not display name or current school', async () => {
  showManagement('accounts')
  expect(await screen.findByText('ordinary')).toBeTruthy()
  expect(screen.queryByLabelText('当前学校')).toBeNull()
  expect(screen.queryByText('平台维护者')).toBeNull()
  expect(screen.queryByRole('button', {name:'新建学校'})).toBeNull()
  fireEvent.click(screen.getByRole('menuitem', {name:'本地管理员管理'}))
  expect(await screen.findByText('平台维护者')).toBeTruthy()
  expect(screen.queryByText('ordinary')).toBeNull()
  expect(screen.queryByRole('button', {name:'新建账号'})).toBeNull()
  expect(screen.queryByRole('button', {name:'编辑账号'})).toBeNull()
  fireEvent.click(screen.getByRole('button', {name:'重置密码'}))
  expect(await screen.findByText(/这是当前登录账号/)).toBeTruthy()
  fireEvent.change(screen.getByLabelText('新密码（至少12个字符）'), {target:{value:'changed-password'}})
  vi.mocked(identity.reset).mockResolvedValue(undefined as never)
  vi.mocked(identity.session).mockRejectedValue(new Error('expired'))
  fireEvent.click(screen.getByRole('button', {name:'确认重置密码'}))
  await waitFor(() => expect(identity.reset).toHaveBeenCalledWith('u','changed-password'))
  expect(await screen.findByRole('heading', {name:'登录工作空间'})).toBeTruthy()
})

it('searches and paginates schools, and opens settings for the selected row', async () => {
  showManagement('schools')
  vi.mocked(workspace.schools).mockResolvedValue(Array.from({length:12}, (_, i) => ({id:`s${i}`,name:`学校-${i}`,active:true,revision:1,config:{}})))
  expect(await screen.findByText('学校-0')).toBeTruthy()
  expect(screen.queryByText('学校-11')).toBeNull()
  fireEvent.click(screen.getByTitle('2'))
  expect(await screen.findByText('学校-11')).toBeTruthy()
  fireEvent.change(screen.getByLabelText('搜索学校'), {target:{value:'学校-11'}})
  const row = screen.getByText('学校-11').closest('tr')!
  fireEvent.click(within(row).getByRole('button', {name:'学校配置'}))
  expect(await screen.findByLabelText('学校名称')).toHaveProperty('value','学校-11')
  expect(screen.getByText('学校-11 · 学校设置')).toBeTruthy()
})

it('shows account grants across schools and saves global account state without a school id', async () => {
  showManagement('accounts')
  vi.mocked(workspace.list).mockImplementation(async school => ({items:[{id:'ordinary',role: school === 'school' ? 'admin' : 'operator'}]}))
  // Scope role queries to the account row/dialog: scanning the entire Ant Design
  // workspace repeatedly computes accessible names and styles for every button.
  const row = within((await screen.findByText('ordinary')).closest('tr')!)
  fireEvent.click(row.getByRole('button', {name:/详\s*情/}))
  expect(await screen.findByText('甲学校 · 学校管理员')).toBeTruthy()
  expect(await screen.findByText('乙学校 · 运营')).toBeTruthy()
  const details = screen.getByRole('dialog')
  fireEvent.click(within(details).getByRole('button', {name:'Close'}))
  await waitFor(() => expect(details.isConnected).toBe(false))
  fireEvent.click(row.getByRole('button', {name:'编辑账号'}))
  const editor = within(screen.getByRole('dialog'))
  await selectOption(editor.getByLabelText('账号状态（全平台）'), '停用')
  fireEvent.click(editor.getByRole('button', {name:/^保\s*存$/}))
  await waitFor(() => expect(identity.update).toHaveBeenCalledWith('ordinary', {display_name:'本地管理员',active:false}))
})

it.each(['member','operator'] as const)('blocks school settings and member administration for %s even through a direct URL', async role => {
  showManagement('settings',false,role)
  await screen.findByLabelText('当前学校')
  await waitFor(() => expect(workspace.schools).toHaveBeenCalled())
  expect(screen.queryByRole('menuitem', {name:'学校设置'})).toBeNull()
  expect(screen.queryByRole('menuitem', {name:'学校成员'})).toBeNull()
  expect(screen.queryByLabelText('学校名称')).toBeNull()
  expect(screen.queryByRole('menuitem', {name:'账号管理'})).toBeNull()
})

it('allows a school administrator to read members without offering grant changes', async () => {
  showManagement('members',false)
  expect(await screen.findByText('学校授权由平台管理员维护。')).toBeTruthy()
  expect(screen.queryByRole('button',{name:'保存所选成员授权'})).toBeNull()
  expect(identity.accounts).not.toHaveBeenCalled()
  await waitFor(() => expect(workspace.list).toHaveBeenCalledWith('school','members',expect.any(String),expect.any(AbortSignal)))
})

it('shows newly created unassigned accounts directly in school members and grants the selected school role', async () => {
  showManagement('members')
  let granted = false
  vi.mocked(workspace.list).mockImplementation(async () => ({items:granted ? [{id:'ordinary',username:'ordinary',display_name:'本地管理员',role:'operator'}] : []}))
  vi.mocked(workspace.mutate).mockImplementation(async () => { granted = true; return {id:'ordinary'} })
  const row = (await screen.findByText('ordinary')).closest('tr')!
  expect(within(row).getByText('未授权')).toBeTruthy()
  expect(screen.getByText('平台维护者')).toBeTruthy()
  fireEvent.click(within(row).getByRole('checkbox'))
  fireEvent.click(screen.getByRole('button', {name:'保存所选成员授权'}))
  await waitFor(() => expect(workspace.mutate).toHaveBeenCalledWith('school','members/ordinary',{role:'operator'},'PUT'))
  await waitFor(() => expect(within(screen.getByText('ordinary').closest('tr')!).getByText('运营')).toBeTruthy())
})

it('allows explicit school administrator grants for platform accounts and refreshes session permissions', async () => {
  showManagement('members')
  vi.mocked(workspace.mutate).mockResolvedValue({id:'u'})
  const row=(await screen.findByText('平台维护者')).closest('tr')!
  fireEvent.click(within(row).getByRole('checkbox'))
  await selectOption(screen.getByRole('combobox',{name:'学校角色'}),'学校管理员')
  const calls=vi.mocked(identity.session).mock.calls.length
  fireEvent.click(screen.getByRole('button',{name:'保存所选成员授权'}))
  await waitFor(()=>expect(workspace.mutate).toHaveBeenCalledWith('school','members/u',{role:'admin'},'PUT'))
  await waitFor(()=>expect(vi.mocked(identity.session).mock.calls.length).toBeGreaterThan(calls))
})

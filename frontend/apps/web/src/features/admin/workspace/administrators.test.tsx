import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Administrators } from './administrators'
import { identity, type User } from './api'

vi.mock('./api', () => ({ identity: { administrators: vi.fn(), createAdministrator: vi.fn(), updateAdministrator: vi.fn(), deleteAdministrator: vi.fn(), resetAdministrator: vi.fn() } }))
const user = {id:'self',username:'root',display_name:'管理员',platform_admin:true} as User
const other = {id:'other',username:'operator',display_name:'其他管理员',active:true,platform_admin:true}
afterEach(() => { cleanup(); vi.resetAllMocks() })
beforeEach(() => {
  vi.mocked(identity.administrators).mockResolvedValue([{...user,active:true},other])
  vi.mocked(identity.createAdministrator).mockResolvedValue(other)
  vi.mocked(identity.updateAdministrator).mockResolvedValue(other)
  vi.mocked(identity.deleteAdministrator).mockResolvedValue(null)
})
function show() {
  render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><Administrators user={user} /></QueryClientProvider>)
}
it('queries by account or name and resets the query', async () => {
  show()
  await screen.findByText('其他管理员')
  fireEvent.change(screen.getByLabelText('查询管理员'), {target:{value:'other & 姓名'}})
  fireEvent.click(screen.getByRole('button',{name:/^查\s*询$/}))
  await waitFor(() => expect(identity.administrators).toHaveBeenCalledWith('other & 姓名'))
  fireEvent.click(screen.getByRole('button',{name:'重置查询'}))
  expect((screen.getByLabelText('查询管理员') as HTMLInputElement).value).toBe('')
  await waitFor(() => expect(identity.administrators).toHaveBeenLastCalledWith(''))
})
it('creates an administrator with a unique account, name and initial password', async () => {
  show()
  fireEvent.click(screen.getByRole('button',{name:'新增管理员'}))
  expect((screen.getByRole('button',{name:'保存管理员'}) as HTMLButtonElement).disabled).toBe(true)
  fireEvent.change(screen.getByLabelText('管理员账号'), {target:{value:'new-admin'}})
  fireEvent.change(screen.getByLabelText('管理员姓名'), {target:{value:'新管理员'}})
  fireEvent.change(screen.getByLabelText('管理员初始密码'), {target:{value:'new-password-123'}})
  fireEvent.click(screen.getByRole('button',{name:'保存管理员'}))
  await waitFor(() => expect(identity.createAdministrator).toHaveBeenCalledWith({username:'new-admin',display_name:'新管理员',password:'new-password-123'}))
  expect(await screen.findByText('管理员已新增')).toBeTruthy()
})
it('edits the name and enabled state without modifying the account name', async () => {
  show()
  const row = (await screen.findByText('其他管理员')).closest('tr')!
  fireEvent.click(within(row).getByRole('button',{name:/编\s*辑/}))
  expect((screen.getByLabelText('管理员账号') as HTMLInputElement).disabled).toBe(true)
  fireEvent.change(screen.getByLabelText('管理员姓名'), {target:{value:'新姓名'}})
  fireEvent.click(screen.getByRole('switch',{name:'管理员启用状态'}))
  fireEvent.click(screen.getByRole('button',{name:'保存管理员'}))
  await waitFor(() => expect(identity.updateAdministrator).toHaveBeenCalledWith('other',{display_name:'新姓名',active:false}))
})
it('requires confirmation to delete and prevents deleting or disabling the current administrator', async () => {
  show()
  const self = (await screen.findByText('root')).closest('tr')!
  expect((within(self).getByRole('button',{name:/删\s*除/}) as HTMLButtonElement).disabled).toBe(true)
  fireEvent.click(within(self).getByRole('button',{name:/编\s*辑/}))
  expect((screen.getByRole('switch',{name:'管理员启用状态'}) as HTMLButtonElement).disabled).toBe(true)
  fireEvent.click(screen.getByRole('button',{name:/取\s*消/}))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  fireEvent.click(within(screen.getByText('其他管理员').closest('tr')!).getByRole('button',{name:/删\s*除/}))
  expect(identity.deleteAdministrator).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button',{name:'确认删除管理员'}))
  await waitFor(() => expect(identity.deleteAdministrator).toHaveBeenCalledWith('other'))
  expect(await screen.findByText('管理员已删除，该账号已停用')).toBeTruthy()
})

import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { VisibleAccountCreate, VisibleAccountReview } from './visible'
import { AccountRegistrationForm, EvidenceField, ProfileForm, ResponsibleActorDisplay, ResponsibleActorSelect } from './account-registration'
import { api, type ResponsibleActor, type SendingAccount } from './api'

vi.mock('./api', () => ({ api: { responsibleActors: vi.fn(), createAccount: vi.fn(), reviewAccountProfile: vi.fn(), updateAccountProfile: vi.fn() } }))
vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} })
Object.defineProperty(window, 'matchMedia', { writable: true, value: () => ({ matches: false, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn() }) })
afterEach(() => { cleanup(); vi.resetAllMocks() })
function show(children: ReactNode) {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } })}>{children}</QueryClientProvider>)
}
const first: ResponsibleActor = { id: 'user-one', display_name: '张三', username: 'zhangsan1', active: true, eligible: true, schools: [{ id: 'school-a', name: 'A学校' }] }
const second: ResponsibleActor = { ...first, id: 'user-two', username: 'zhangsan2', schools: [{ id: 'school-b', name: 'B学校' }] }
const account: SendingAccount = {
  id: 'sender', revision: 3, client_kind: 'wecom', identity_mode: 'ui_visible', identity_namespace: null, external_key: null,
  visible_profile: { company_name: '测试企业', account_name: '测试账号' }, profile_revision: 2, verification_state: 'pending',
  responsible_actor_id: first.id, responsible_actor: first, registration_evidence_ref: '手机登记来源', identity_evidence_ref: '旧核对依据',
  latest_profile_review: { actor_id: 'reviewer', actor_name: '李四', actor_username: 'lisi', reviewed_at: '2026-10-31T02:00:00Z', profile_revision: 1, evidence_ref: '旧核对依据' },
  enabled: true, daily_limit: 30, verified_at: '',
}

it('loads candidates on opening, searches users, retains selection across pages and submits the ID', async () => {
  vi.mocked(api.responsibleActors).mockImplementation(async (_search, page) => ({ items: page === 2 ? [second] : [first], page: { number: page ?? 1, size: 30, has_more: page !== 2 } }))
  vi.mocked(api.createAccount).mockResolvedValue(account)
  const saved = vi.fn().mockResolvedValue(undefined)
  show(<VisibleAccountCreate saved={saved} />)
  expect(api.responsibleActors).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: '登记企业微信界面账号' }))
  await waitFor(() => expect(api.responsibleActors).toHaveBeenCalledWith('', 1))
  expect(screen.queryByRole('textbox', { name: /责任人用户 ID/ })).toBeNull()
  const select = screen.getByRole('combobox', { name: '本系统责任人' })
  fireEvent.mouseDown(select)
  fireEvent.change(select, { target: { value: 'zhangsan' } })
  await waitFor(() => expect(api.responsibleActors).toHaveBeenCalledWith('zhangsan', 1))
  fireEvent.click(await screen.findByText('张三（zhangsan1） · A学校管理员'))
  fireEvent.click(screen.getByRole('button', { name: '下一页责任人' }))
  await waitFor(() => expect(api.responsibleActors).toHaveBeenCalledWith('zhangsan', 2))
  expect(screen.getAllByText('张三（zhangsan1） · A学校管理员').length).toBeGreaterThan(0)
  fireEvent.change(screen.getByRole('textbox', { name: '企业微信中可见企业名称' }), { target: { value: '测试企业' } })
  fireEvent.change(screen.getByRole('textbox', { name: '企业微信中可见账号名称' }), { target: { value: '测试账号' } })
  fireEvent.change(screen.getByRole('textbox', { name: '登记依据' }), { target: { value: '  手机企业与账号页面  ' } })
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await waitFor(() => expect(saved).toHaveBeenCalledOnce())
  expect(api.createAccount).toHaveBeenCalledWith({ client_kind: 'wecom', identity_mode: 'ui_visible', visible_profile: { company_name: '测试企业', account_name: '测试账号', account_detail_text: null }, responsible_actor_id: first.id, identity_evidence_ref: '手机企业与账号页面' }, expect.any(String))
  expect(api.reviewAccountProfile).not.toHaveBeenCalled()
})

it('explains empty candidates and supports retry without exposing a free ID input', async () => {
  vi.mocked(api.responsibleActors).mockRejectedValueOnce(new Error('无平台管理权限')).mockResolvedValueOnce({ items: [] })
  show(<ResponsibleActorSelect />)
  await screen.findByText('无平台管理权限')
  fireEvent.click(screen.getByRole('button', { name: '重试查询' }))
  await screen.findByText(/暂无已启用且具有学校管理员授权的账号。请在账号管理/)
  expect(screen.getByRole('link', { name: '前往账号管理' }).getAttribute('href')).toBe('/admin?view=accounts')
  expect(screen.queryByRole('textbox')).toBeNull()
})

it('does not automatically select an eligible user for legacy registration', async () => {
  vi.mocked(api.responsibleActors).mockResolvedValue({ items: [first, second] })
  show(<AccountRegistrationForm legacy saved={vi.fn()} />)
  await waitFor(() => expect(api.responsibleActors).toHaveBeenCalled())
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await screen.findByText('请选择本系统责任人')
  expect(api.createAccount).not.toHaveBeenCalled()
})

it('rejects blank and overlong evidence while counting Unicode characters like Rust', async () => {
  const save = vi.fn().mockResolvedValue(undefined)
  show(<ProfileForm onSave={save}><EvidenceField /></ProfileForm>)
  const field = screen.getByRole('textbox', { name: '本次核对依据' })
  for (const value of ['  ', '字'.repeat(501)]) {
    fireEvent.change(field, { target: { value } })
    fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
    await screen.findByText('本次核对依据需为 1–500 字，不能仅填写空白')
    expect(save).not.toHaveBeenCalled()
  }
  fireEvent.change(field, { target: { value: '𠮷'.repeat(500) } })
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await waitFor(() => expect(save).toHaveBeenCalledWith({ evidence_ref: '𠮷'.repeat(500) }, expect.any(String)))
})

it('keeps the same idempotency key after an ambiguous submission', async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error('响应丢失')).mockResolvedValueOnce(undefined)
  show(<ProfileForm onSave={save}><EvidenceField /></ProfileForm>)
  fireEvent.change(screen.getByRole('textbox', { name: '本次核对依据' }), { target: { value: '现场核对企业及账号页面' } })
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await screen.findByText('响应丢失')
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await screen.findByText('已保存')
  expect(save.mock.calls[0]).toEqual(save.mock.calls[1])
})

it('shows historical evidence and requires a new explicit review for the displayed revision', async () => {
  vi.mocked(api.reviewAccountProfile).mockRejectedValueOnce(Object.assign(new Error('revision_conflict'), { code: 'revision_conflict' }))
  show(<VisibleAccountReview account={account} saved={vi.fn()} />)
  expect(screen.getByText('待人工核对')).toBeTruthy()
  expect(screen.getByText(/历史核对，不适用于当前待核对资料/)).toBeTruthy()
  expect(screen.getByText('登记依据：手机登记来源')).toBeTruthy()
  const field = screen.getByRole('textbox', { name: '本次核对依据' }) as HTMLTextAreaElement
  expect(field.value).toBe('')
  fireEvent.change(field, { target: { value: '已在当前手机核对企业及账号页面' } })
  fireEvent.click(screen.getByRole('button', { name: '已在手机核对本版本账号资料' }))
  await screen.findByText('资料版本已变化，请刷新当前资料后重新核对。')
  expect(api.reviewAccountProfile).toHaveBeenCalledWith(account.id, { expected_revision: 3, evidence_ref: '已在当前手机核对企业及账号页面' }, expect.any(String))
})

it('displays historical owners independently of candidate pages and marks disabled users', () => {
  show(<ResponsibleActorDisplay account={{ ...account, responsible_actor: { ...first, active: false, eligible: false } }} />)
  expect(screen.getByText(/张三（zhangsan1）/)).toBeTruthy()
  expect(screen.getByText('账号已停用')).toBeTruthy()
  expect(api.responsibleActors).not.toHaveBeenCalled()
  expect(screen.queryByText(first.id)).toBeNull()
})

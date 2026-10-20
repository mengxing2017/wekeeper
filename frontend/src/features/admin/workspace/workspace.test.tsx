import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Workspace } from './index'
import { identity, workspace } from './api'
vi.mock('./api', () => ({ identity: { session: vi.fn(), accounts: vi.fn(), logout: vi.fn() }, workspace: { schools: vi.fn(), list: vi.fn(), createSchool: vi.fn() } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
Object.defineProperty(window, 'matchMedia', { writable: true, value: vi.fn().mockImplementation(query => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })) })
it('restores the selected school and business page from its URL', async () => {
  vi.mocked(identity.session).mockResolvedValue({csrf:'csrf',user:{id:'u',username:'admin',display_name:'管理员',platform_admin:true,bound_school_id:null,grants:[],wecom_authenticated:false}})
  vi.mocked(workspace.schools).mockResolvedValue([{id:'school',name:'测试学校',active:true,revision:1,config:{}}])
  vi.mocked(workspace.list).mockResolvedValue([])
  vi.mocked(identity.accounts).mockResolvedValue([])
  const client = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={client}><MemoryRouter initialEntries={['/admin?school=school&view=groups']}><Workspace/></MemoryRouter></QueryClientProvider>)
  expect(await screen.findByRole('heading', { name: '客户群', level: 1 })).toBeTruthy()
  await waitFor(() => expect(workspace.list).toHaveBeenCalledWith('school','groups',expect.any(String),expect.any(AbortSignal)))
  fireEvent.click(screen.getByRole('button', { name: '学校与账号' }))
  expect(await screen.findByRole('heading', { name: '学校与账号', level: 1 })).toBeTruthy()
  expect(await screen.findByLabelText('学校名称')).toBeTruthy()
})

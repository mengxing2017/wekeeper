import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { SessionGate } from './index'
import { identity } from './api'

vi.mock('./api', () => ({ identity: { session: vi.fn(), login: vi.fn() }, workspace: {} }))
afterEach(cleanup)
it('validates login fields and submits the entered credentials through the existing client', async () => {
  vi.mocked(identity.session).mockRejectedValue(new Error('未登录'))
  vi.mocked(identity.login).mockResolvedValue({ csrf: 'csrf', user: { id: 'u', username: 'operator', display_name: '运营', platform_admin: false, bound_school_id: null, grants: [], wecom_authenticated: false } })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><SessionGate>{user => <p>欢迎，{user.display_name}</p>}</SessionGate></QueryClientProvider>)
  fireEvent.click(await screen.findByRole('button', { name: /^登\s*录$/ }))
  await screen.findByText('请填写账号')
  expect(identity.login).not.toHaveBeenCalled()
  fireEvent.change(screen.getByLabelText('账号'), { target: { value: 'operator' } })
  fireEvent.change(screen.getByLabelText('密码'), { target: { value: 'test-password' } })
  fireEvent.click(screen.getByRole('button', { name: /^登\s*录$/ }))
  await waitFor(() => expect(identity.login).toHaveBeenCalledWith('operator', 'test-password'))
  expect(await screen.findByText('欢迎，运营')).toBeTruthy()
})

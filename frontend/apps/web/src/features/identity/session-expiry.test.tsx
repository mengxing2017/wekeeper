import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query'
import { SessionGate } from './index'
import { identity, type User } from './api'

const user: User = {
  id: 'user', username: 'operator', display_name: '运营', platform_admin: false,
  bound_school_id: null, grants: [], wecom_authenticated: false,
}
afterEach(() => { cleanup(); vi.unstubAllGlobals() })

function mount(status: number, background = false) {
  const fetcher = vi.fn().mockImplementation(async () => Response.json(
    { errors: [{ status: String(status), code: status === 401 ? 'unauthorized' : 'forbidden', title: 'Error', detail: '请求被拒绝' }] },
    { status, headers: { 'Content-Type': 'application/vnd.api+json' } },
  ))
  vi.stubGlobal('fetch', fetcher)
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
  cache.setQueryData(['session'], { csrf: 'old-csrf', user })
  cache.setQueryData(['private-data'], ['private school results'])
  function BackgroundRequest() {
    useQuery({ queryKey: ['protected-refresh'], queryFn: identity.session, retry: false })
    return <p>后台加载中</p>
  }
  render(<QueryClientProvider client={cache}><SessionGate>{() => <>
    <p>受保护工作区</p>
    {background && <BackgroundRequest />}
    <button onClick={() => { void identity.logout().catch(() => {}) }}>提交业务操作</button>
  </>}</SessionGate></QueryClientProvider>)
  return { cache, fetcher }
}

it('returns to login and clears private caches when a background request gets 401', async () => {
  const { cache, fetcher } = mount(401, true)
  expect(await screen.findByRole('heading', { name: '登录工作空间' })).toBeTruthy()
  expect(screen.queryByText('受保护工作区')).toBeNull()
  expect(cache.getQueryData(['private-data'])).toBeUndefined()
  expect(cache.getQueryData(['session'])).toBeNull()
  expect(fetcher).toHaveBeenCalledTimes(1)
})

it('returns to login for directly awaited writes and allows logging in again', async () => {
  const { fetcher } = mount(401)
  fireEvent.click(screen.getByRole('button', { name: '提交业务操作' }))
  expect(await screen.findByRole('heading', { name: '登录工作空间' })).toBeTruthy()
  expect(screen.getByText('登录已失效，请重新登录')).toBeTruthy()
  fetcher.mockResolvedValue(Response.json(
    { data: { type: 'sessions', id: 'current', attributes: { csrf: 'new-csrf', user } } },
    { headers: { 'Content-Type': 'application/vnd.api+json' } },
  ))
  fireEvent.change(screen.getByLabelText('账号'), { target: { value: 'operator' } })
  fireEvent.change(screen.getByLabelText('密码'), { target: { value: 'password' } })
  fireEvent.click(screen.getByRole('button', { name: /^登\s*录$/ }))
  expect(await screen.findByText('受保护工作区')).toBeTruthy()
  expect(screen.queryByText('登录已失效，请重新登录')).toBeNull()
})

it('keeps the session when a request is forbidden rather than unauthenticated', async () => {
  const { cache, fetcher } = mount(403)
  fireEvent.click(screen.getByRole('button', { name: '提交业务操作' }))
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1))
  expect(screen.getByText('受保护工作区')).toBeTruthy()
  expect(cache.getQueryData(['session'])).toEqual({ csrf: 'old-csrf', user })
  expect(screen.queryByRole('heading', { name: '登录工作空间' })).toBeNull()
})

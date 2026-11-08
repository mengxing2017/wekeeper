import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppRoutes } from './routes'

afterEach(() => { cleanup(); vi.unstubAllGlobals() })

it('loads the actual admin entry and displays login when unauthenticated', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({
    errors: [{ status: '401', code: 'unauthorized', detail: '请先登录' }],
  }, { status: 401 })))
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={cache}><MemoryRouter initialEntries={['/admin']}><AppRoutes /></MemoryRouter></QueryClientProvider>)
  expect(await screen.findByRole('heading', { name: '登录工作空间' }, { timeout: 15000 })).toBeTruthy()
  expect(screen.queryByText('页面暂时无法加载')).toBeNull()
})

it('loads the authenticated admin entry with independent workspaces', async () => {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (url.endsWith('/auth/session')) return Response.json({ data: {
      type: 'sessions', id: 'current', attributes: { csrf: 'csrf', user: {
        id: 'admin', username: 'admin', display_name: '系统管理员', platform_admin: true,
        bound_school_id: null, grants: [], wecom_authenticated: false,
      } },
    } }, { headers: { 'Content-Type': 'application/vnd.api+json' } })
    if (url.endsWith('/workspaces')) return Response.json({ data: [] }, { headers: { 'Content-Type': 'application/vnd.api+json' } })
    throw new Error(`Unexpected request: ${url}`)
  }))
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={cache}><MemoryRouter initialEntries={['/admin']}><AppRoutes /></MemoryRouter></QueryClientProvider>)
  expect(await screen.findByText('暂无企业微信分发授权，请联系系统管理员开通工作空间权限。', {}, { timeout: 15000 })).toBeTruthy()
  expect(screen.queryByLabelText('企业微信分发工作空间')).toBeNull()
  expect(screen.queryByRole('button', { name: '新建企业微信工作空间' })).toBeNull()
  expect(screen.queryByText('页面暂时无法加载')).toBeNull()
})

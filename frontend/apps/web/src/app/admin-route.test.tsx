import { afterEach, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { createMemoryRouter, MemoryRouter, RouterProvider } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppRoutes } from './routes'

afterEach(() => { cleanup(); vi.unstubAllGlobals() })

it('loads the actual admin entry and displays login when unauthenticated', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({
    errors: [{ status: '401', code: 'unauthorized', detail: '请先登录' }],
  }, { status: 401 })))
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={cache}><MemoryRouter initialEntries={['/admin?view=groups']}><AppRoutes /></MemoryRouter></QueryClientProvider>)
  expect(await screen.findByRole('heading', { name: '登录管理后台' }, { timeout: 15000 })).toBeTruthy()
  expect(screen.queryByText('页面暂时无法加载')).toBeNull()
})

it('loads authenticated global administrator entry', async () => {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (url.endsWith('/auth/session')) return Response.json({ data: {
      type: 'sessions', id: 'current', attributes: { csrf: 'csrf', user: {
        id: 'admin', username: 'admin', display_name: '系统管理员', platform_admin: true, wecom_authenticated: false,
      } },
    } }, { headers: { 'Content-Type': 'application/vnd.api+json' } })
    if (url.includes('/wecom-distribution/groups')) return Response.json({ data: [] }, { headers: { 'Content-Type': 'application/vnd.api+json' } })
    throw new Error(`Unexpected request: ${url}`)
  }))
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={cache}><MemoryRouter initialEntries={['/admin?view=groups']}><AppRoutes /></MemoryRouter></QueryClientProvider>)
  expect(await screen.findByRole('heading', { name: '客户群' }, { timeout: 15000 })).toBeTruthy()
  expect(screen.queryByLabelText('企业微信分发工作空间')).toBeNull()
  expect(screen.queryByRole('button', { name: '新建企业微信工作空间' })).toBeNull()
  expect(screen.queryByText('页面暂时无法加载')).toBeNull()
})

it.each([
  {old: 'advertisement-rotations', canonical: 'advertisements', title: '广告素材与投放', button: '绑定投放范围'},
  {old: 'execution-devices&subview=usage', canonical: 'shared-mobile-resources', title: '手机群消息使用管理', button: '开通'},
])('replaces legacy $old before rendering, including browser history and navigation state', async ({old, canonical, title, button}) => {
  const requests: string[] = []
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    requests.push(url)
    const reply = (attributes: unknown) => Response.json({data: {type: 'result', id: 'result', attributes}}, {headers: {'Content-Type': 'application/vnd.api+json'}})
    if (url.endsWith('/auth/session')) return reply({csrf: 'csrf', user: {
      id: 'admin', username: 'admin', display_name: '系统管理员', platform_admin: true, wecom_authenticated: false,
    }})
    if (url.includes('/wecom-distribution/groups')) return Response.json({data: []}, {headers: {'Content-Type': 'application/vnd.api+json'}})
    if (url.includes('/usage-options')) return reply({devices: [], targets: [], scheme: null})
    if (url.endsWith('/usages')) return reply({items: []})
    if (/\/(ad-materials|ad-placements|business-platforms)(\?|$)/.test(url)) return reply({items: [], has_more: false})
    throw new Error(`Unexpected request: ${url}`)
  }))
  const router = createMemoryRouter([{path: '*', element: <AppRoutes />}], {
    initialEntries: ['/admin?view=groups', `/admin?view=${old}&keep=value%20with%20space#section`], initialIndex: 1,
  })
  const cache = new QueryClient({defaultOptions: {queries: {retry: false}}})
  render(<QueryClientProvider client={cache}><RouterProvider router={router}/></QueryClientProvider>)
  expect(await screen.findByRole('heading', {name: title, level: 1}, {timeout: 15000})).toBeTruthy()
  expect(new URLSearchParams(router.state.location.search).get('view')).toBe(canonical)
  expect(new URLSearchParams(router.state.location.search).get('subview')).toBeNull()
  expect(new URLSearchParams(router.state.location.search).get('keep')).toBe('value with space')
  expect(router.state.location.hash).toBe('#section')
  expect(router.state.historyAction).toBe('REPLACE')
  expect(screen.getByRole('menuitem', {name: title}).getAttribute('aria-current')).toBe('page')
  expect(screen.getByRole('button', {name: new RegExp(button.split('').join('\\s*'))})).toBeTruthy()
  expect(screen.queryByText('页面不存在')).toBeNull()
  expect(screen.queryByRole('menuitem', {name: '广告序列轮询'})).toBeNull()
  expect(requests.some(url => url.includes('/devices'))).toBe(false)
  fireEvent.click(screen.getByRole('button', {name: '收起侧栏'}))
  fireEvent.click(screen.getByRole('button', {name: '打开导航'}))
  expect(screen.getByRole('button', {name: '关闭导航'})).toBeTruthy()
  await waitFor(() => expect(screen.getByRole('menuitem', {name: title}).getAttribute('aria-current')).toBe('page'))
  fireEvent.keyDown(window, {key: 'Escape'})
  await act(async () => { await router.navigate(-1) })
  expect(await screen.findByRole('heading', {name: '客户群', level: 1})).toBeTruthy()
  expect(router.state.location.search).toBe('?view=groups')
  await act(async () => { await router.navigate(1) })
  await waitFor(() => expect(screen.getByRole('heading', {name: title, level: 1})).toBeTruthy())
  expect(new URLSearchParams(router.state.location.search).get('view')).toBe(canonical)
  router.dispose()
})

import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppRoutes } from './routes'
import { PageErrorBoundary } from './page-error-boundary'

afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks() })
it('loads the real lazy WeCom route and contract without mocking feature modules', async () => {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => url.endsWith('/wecom/status')
    ? Response.json({data:{type:'wecom-status',id:'current',attributes:{server_configured:false,client_configured:false,oauth_available:false,share_available:false}}},{headers:{'Content-Type':'application/vnd.api+json'}})
    : Response.json({errors:[{status:'401',code:'unauthorized',title:'Unauthorized',detail:'请先登录'}]},{status:401})))
  const cache = new QueryClient({defaultOptions:{queries:{retry:false}}})
  render(<QueryClientProvider client={cache}><MemoryRouter initialEntries={['/wecom']}><AppRoutes/></MemoryRouter></QueryClientProvider>)
  expect(await screen.findByRole('heading',{name:'分享入口已迁移'}, { timeout: 5000 })).toBeTruthy()
  expect(await screen.findByText('登录运营后台查看群发任务')).toBeTruthy()
})
it('renders recovery controls instead of a blank page on a render failure', () => {
  vi.spyOn(console,'error').mockImplementation(()=>{})
  function Broken(): never { throw new Error('chunk or render failure') }
  render(<PageErrorBoundary><Broken/></PageErrorBoundary>)
  expect(screen.getByRole('alert')).toBeTruthy()
  expect(screen.getByRole('button',{name:'重新加载'})).toBeTruthy()
})

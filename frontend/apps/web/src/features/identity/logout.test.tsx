import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query'
import { SessionGate } from './index'
import { type User } from './api'

const user: User = {id:'admin',username:'admin',display_name:'管理员',platform_admin:true,bound_school_id:null,grants:[],wecom_authenticated:false}
const headers = {'Content-Type':'application/vnd.api+json'}
afterEach(() => { cleanup(); vi.unstubAllGlobals() })
function mount(background?: React.ReactNode) {
  const cache = new QueryClient({defaultOptions:{queries:{retry:false,staleTime:Infinity}}})
  cache.setQueryData(['session'], {csrf:'csrf',user})
  cache.setQueryData(['private-data'], ['private results'])
  render(<QueryClientProvider client={cache}><SessionGate>{(_, logout) => <>
    <p>受保护工作区</p>{background}
    <button onClick={() => { void logout() }}>测试退出入口</button>
  </>}</SessionGate></QueryClientProvider>)
  return cache
}
it('shows login immediately after logout succeeds without a second session round trip', async () => {
  const fetcher = vi.fn((_: string, options?: RequestInit) => options?.method === 'DELETE'
    ? Promise.resolve(Response.json({data:null},{headers}))
    : new Promise<Response>(() => {}))
  vi.stubGlobal('fetch', fetcher)
  const cache = mount()
  fireEvent.click(screen.getByRole('button',{name:'退出登录'}))
  expect(await screen.findByRole('heading',{name:'登录工作空间'})).toBeTruthy()
  expect(fetcher).toHaveBeenCalledTimes(1)
  expect(fetcher.mock.calls[0][1]?.method).toBe('DELETE')
  expect(cache.getQueryData(['session'])).toBeNull()
  expect(cache.getQueryData(['private-data'])).toBeUndefined()
})
it('cancels protected queries before submitting logout', async () => {
  let backgroundSignal: AbortSignal | undefined
  function Background() {
    useQuery({queryKey:['protected-refresh'],queryFn:({signal}) => {
      backgroundSignal = signal
      return new Promise(() => {})
    }})
    return null
  }
  const fetcher = vi.fn(async () => {
    expect(backgroundSignal?.aborted).toBe(true)
    return Response.json({data:null},{headers})
  })
  vi.stubGlobal('fetch',fetcher)
  mount(<Background />)
  fireEvent.click(screen.getByRole('button',{name:'退出登录'}))
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1))
  expect(await screen.findByRole('heading',{name:'登录工作空间'})).toBeTruthy()
})
it('reports failed logout without claiming success and permits retry', async () => {
  const fetcher = vi.fn().mockRejectedValueOnce(new Error('network unavailable')).mockImplementation(async () => Response.json({data:null},{headers}))
  vi.stubGlobal('fetch',fetcher)
  const cache = mount()
  fireEvent.click(screen.getByRole('button',{name:'退出登录'}))
  expect(await screen.findByText('网络中断，请刷新记录核对操作结果')).toBeTruthy()
  expect(screen.getByText('受保护工作区')).toBeTruthy()
  expect(cache.getQueryData(['session'])).toEqual({csrf:'csrf',user})
  fireEvent.click(screen.getByRole('button',{name:'退出登录'}))
  expect(await screen.findByRole('heading',{name:'登录工作空间'})).toBeTruthy()
  expect(fetcher).toHaveBeenCalledTimes(2)
})
it('deduplicates logout while awaiting revocation and does not exit before success', async () => {
  let finish!: (value: Response) => void
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve }))
  vi.stubGlobal('fetch',fetcher)
  mount()
  const exit = screen.getByRole('button',{name:'测试退出入口'})
  fireEvent.click(exit)
  fireEvent.click(exit)
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1))
  expect(screen.getByText('受保护工作区')).toBeTruthy()
  finish(Response.json({data:null},{headers}))
  expect(await screen.findByRole('heading',{name:'登录工作空间'})).toBeTruthy()
})

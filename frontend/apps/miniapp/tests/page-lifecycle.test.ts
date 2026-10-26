import { expect, it, vi } from 'vitest'
const hooks = vi.hoisted(() => ({
  onReady: vi.fn(),
  onShow: vi.fn(),
  onHide: vi.fn(),
  onPullDownRefresh: vi.fn(),
  onReachBottom: vi.fn(),
  onShareAppMessage: vi.fn(),
}))
vi.mock('@dcloudio/uni-app', () => hooks)
import { useScreen } from '../src/app/page-lifecycle'
it('bridges registered page lifecycle events to feature components after refs become ready', async () => {
  const screen = useScreen(true)
  hooks.onShow.mock.calls[0][0]() // first onShow precedes child creation
  const show = vi.fn(),
    hide = vi.fn(),
    loadMore = vi.fn(),
    refresh = vi.fn().mockResolvedValue(undefined)
  const stopPullDownRefresh = vi.fn()
  vi.stubGlobal('uni', { stopPullDownRefresh })
  screen.value = {
    show,
    hide,
    loadMore,
    refresh,
    share: () => ({ title: 'Campus', path: '/pages/index/detail?school=s&id=p' }),
  }
  hooks.onReady.mock.calls[0][0]()
  expect(show).toHaveBeenCalledTimes(1)
  hooks.onShow.mock.calls[0][0]()
  expect(show).toHaveBeenCalledTimes(2)
  hooks.onHide.mock.calls[0][0]()
  expect(hide).toHaveBeenCalledOnce()
  hooks.onReachBottom.mock.calls[0][0]()
  expect(loadMore).toHaveBeenCalledOnce()
  hooks.onPullDownRefresh.mock.calls[0][0]()
  await Promise.resolve()
  expect(refresh).toHaveBeenCalledOnce()
  expect(stopPullDownRefresh).toHaveBeenCalledOnce()
  expect(hooks.onShareAppMessage.mock.calls[0][0]()).toEqual({
    title: 'Campus',
    path: '/pages/index/detail?school=s&id=p',
  })
  vi.unstubAllGlobals()
})

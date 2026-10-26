import { shallowRef } from 'vue'
import {
  onReady,
  onShow,
  onHide,
  onPullDownRefresh,
  onReachBottom,
  onShareAppMessage,
} from '@dcloudio/uni-app'
interface Screen {
  show?: () => void | Promise<void>
  hide?: () => void
  refresh?: () => void | Promise<void>
  loadMore?: () => void | Promise<void>
  share?: () => { title: string; path: string }
}
/** Uni page hooks belong to registered page wrappers, not child components. */
export function useScreen(share = false) {
  const screen = shallowRef<Screen>()
  onReady(() => {
    void screen.value?.show?.()
  })
  onShow(() => {
    void screen.value?.show?.()
  })
  onHide(() => screen.value?.hide?.())
  onPullDownRefresh(() => {
    Promise.resolve(screen.value?.refresh?.()).finally(() => uni.stopPullDownRefresh())
  })
  onReachBottom(() => {
    void screen.value?.loadMore?.()
  })
  if (share)
    onShareAppMessage(
      () => screen.value?.share?.() ?? { title: 'WeKeeper', path: '/pages/index/index' },
    )
  return screen
}

import { onUnmounted, ref } from 'vue'
import { onScopeChange, clearScope } from '../features/school-context/context'
export function usePage(clear: () => void = () => {}) {
  const busy = ref(false),
    error = ref('')
  let generation = 0
  const stop = onScopeChange(() => {
    generation++
    busy.value = false
    error.value = ''
    clear()
  })
  onUnmounted(() => {
    generation++
    stop()
  })
  async function run(action: (current: () => boolean) => Promise<void>) {
    if (busy.value) return
    const turn = ++generation
    const current = () => turn === generation
    busy.value = true
    error.value = ''
    try {
      await action(current)
    } catch (e) {
      if (current()) {
        if (e && typeof e === 'object' && 'status' in e && e.status === 403) clearScope()
        error.value = e instanceof Error ? e.message : '操作失败，请重试'
      }
    } finally {
      if (turn === generation) busy.value = false
    }
  }
  return { busy, error, run }
}
export function confirm(content: string) {
  return new Promise<boolean>((resolve) =>
    uni.showModal({
      title: '请确认',
      content,
      success: (r) => resolve(r.confirm),
      fail: () => resolve(false),
    }),
  )
}
export function navigate(url: string) {
  uni.navigateTo({ url })
}

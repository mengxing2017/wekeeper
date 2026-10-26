const KEY = 'wekeeper:miniapp:session'
let token = ''
let revision = 0
const listeners = new Set<() => void>()
export const credentials = {
  get token() {
    return token
  },
  get revision() {
    return revision
  },
  restore() {
    const saved: unknown = uni.getStorageSync(KEY)
    token = typeof saved === 'string' ? saved : ''
  },
  set(value: string) {
    token = value
    revision++
    if (value) uni.setStorageSync(KEY, value)
    else uni.removeStorageSync(KEY)
    listeners.forEach((fn) => fn())
  },
  subscribe(fn: () => void) {
    listeners.add(fn)
    return () => listeners.delete(fn)
  },
}

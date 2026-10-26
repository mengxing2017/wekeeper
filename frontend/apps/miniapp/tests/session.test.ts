import { beforeEach, afterEach, it, expect, vi } from 'vitest'
const api = vi.hoisted(() => ({
  miniapp: vi.fn(),
  session: vi.fn(),
  logout: vi.fn(),
  profile: vi.fn(),
}))
vi.mock('../src/features/identity/api', () => ({ identityApi: api }))
import { session, login, restoreSession, logout } from '../src/features/identity/session'
import { credentials } from '../src/app/credentials'
const user = {
  id: 'u',
  username: 'member',
  display_name: '成员',
  bound_school_id: 's',
  grants: [],
  platform_admin: false,
  wecom_authenticated: false,
}
beforeEach(() => {
  vi.stubGlobal('uni', {
    login: vi.fn().mockResolvedValue({ code: 'fresh' }),
    getStorageSync: vi.fn(),
    setStorageSync: vi.fn(),
    removeStorageSync: vi.fn(),
  })
  credentials.set('')
  session.ready = false
  session.error = ''
  api.miniapp.mockReset()
  api.session.mockReset()
  api.logout.mockReset()
})
afterEach(() => vi.unstubAllGlobals())
it('coalesces concurrent interactive logins and persists only the resulting session', async () => {
  api.miniapp.mockResolvedValue({ token: 'new', user })
  await Promise.all([login(), login()])
  expect(uni.login).toHaveBeenCalledOnce()
  expect(api.miniapp).toHaveBeenCalledOnce()
  expect(api.miniapp).toHaveBeenCalledWith('fresh')
  expect(credentials.token).toBe('new')
  expect(session.user?.id).toBe('u')
})
it('does not accept a late login result after logout', async () => {
  let finish: (value: unknown) => void = () => {}
  vi.mocked(uni.login).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve
    }) as never,
  )
  const pending = login()
  api.logout.mockResolvedValue(null)
  await logout()
  finish({ code: 'late' })
  await expect(pending).rejects.toThrow('已取消')
  expect(api.miniapp).not.toHaveBeenCalled()
  expect(credentials.token).toBe('')
})
it('allows restore retry after a transient network error and does not invent an authenticated user', async () => {
  credentials.set('saved')
  api.session.mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce({ user, csrf: '' })
  await expect(restoreSession()).rejects.toThrow('offline')
  expect(session.user).toBeNull()
  expect(session.ready).toBe(false)
  await restoreSession()
  expect(session.user?.id).toBe('u')
  expect(api.session).toHaveBeenCalledTimes(2)
})
it('clears local credentials even when server logout cannot be confirmed', async () => {
  credentials.set('saved')
  session.user = user
  api.logout.mockRejectedValue(new Error('offline'))
  await expect(logout()).rejects.toThrow('offline')
  expect(credentials.token).toBe('')
  expect(session.user).toBeNull()
})

import { reactive } from 'vue'
import { credentials } from '../../app/credentials'
import { identityApi, type User } from './api'
export const session = reactive({ user: null as User | null, ready: false, busy: false, error: '' })
let restoring: Promise<void> | undefined
let loggingIn: Promise<void> | undefined
credentials.subscribe(() => {
  session.user = null
  session.ready = false
})
export async function restoreSession() {
  if (session.ready) return
  if (!restoring)
    restoring = (async () => {
      if (!credentials.token) {
        session.ready = true
        return
      }
      const revision = credentials.revision
      try {
        const value = await identityApi.session()
        if (revision === credentials.revision) session.user = value.user
      } catch (error) {
        session.error = error instanceof Error ? error.message : '恢复登录失败'
        throw error
      } finally {
        session.ready = !!session.user || !credentials.token
      }
    })().finally(() => {
      restoring = undefined
    })
  return restoring
}
export function login() {
  if (!loggingIn)
    loggingIn = (async () => {
      session.busy = true
      session.error = ''
      const revision = credentials.revision
      try {
        const result = await uni.login({ provider: 'weixin' })
        if (revision !== credentials.revision) throw new Error('登录操作已取消')
        if (!result.code) throw new Error('微信登录未返回有效凭证，请重试')
        const value = await identityApi.miniapp(result.code)
        if (revision !== credentials.revision) throw new Error('登录操作已取消')
        credentials.set(value.token)
        session.user = value.user
        session.ready = true
      } catch (error) {
        session.error = error instanceof Error ? error.message : '登录失败'
        throw error
      } finally {
        session.busy = false
      }
    })().finally(() => {
      loggingIn = undefined
    })
  return loggingIn
}
export async function refreshUser() {
  const revision = credentials.revision
  const value = await identityApi.session()
  if (revision === credentials.revision) session.user = value.user
}
export async function logout() {
  try {
    await identityApi.logout()
  } finally {
    credentials.set('')
    session.ready = true
  }
}
export function requireLogin() {
  if (!session.user) {
    uni.navigateTo({ url: '/pages/user/login' })
    return false
  }
  return true
}

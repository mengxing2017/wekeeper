import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Form, Input } from 'antd'
import { Field } from '../../shared/field'
import { identity, subscribeUnauthorized, type User } from './api'
export function SessionGate({
  children,
  wecom = false,
  showSessionBar = true,
}: {
  children: (user: User, logout: () => Promise<void>) => ReactNode
  showSessionBar?: boolean
  wecom?: boolean
}) {
  const cache = useQueryClient()
  const q = useQuery({ queryKey: ['session'], queryFn: identity.session, retry: false })
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const logoutRequest = useRef<Promise<void> | null>(null)
  useEffect(() => subscribeUnauthorized(() => {
    // Initial login failures already render the form. Expiry must also discard
    // cached session data and protected results, including background requests.
    if (!cache.getQueryData(['session'])) return
    void cache.cancelQueries()
    cache.clear()
    cache.setQueryData(['session'], null)
    setError('登录已失效，请重新登录')
  }), [cache])
  async function login(values: { username: string; password: string }) {
    setError('')
    setBusy(true)
    try {
      const s = await identity.login(values.username, values.password)
      await cache.cancelQueries()
      cache.clear()
      cache.setQueryData(['session'], s)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  function logout(): Promise<void> {
    if (logoutRequest.current) return logoutRequest.current
    setError('')
    setBusy(true)
    const request = (async () => {
      try {
        // Stop protected refreshes before revoking the session.
        await cache.cancelQueries()
        await identity.logout()
        // Notify the active session observer; clearing the cache alone leaves
        // its last authenticated result on screen until a later render.
        cache.setQueryData(['session'], null)
        cache.removeQueries({ predicate: query => query.queryKey[0] !== 'session' })
        cache.getMutationCache().clear()
      } catch (e) {
        setError((e as Error).message)
      } finally {
        logoutRequest.current = null
        setBusy(false)
      }
    })()
    logoutRequest.current = request
    return request
  }
  if (q.isPending)
    return (
      <div className="loading" role="status">
        正在确认登录状态…
      </div>
    )
  if (!q.data)
    return (
      <div className="auth-page">
        <section className="auth-panel">
          <div className="auth-brand">
            <span className="brand-mark">W</span> WeKeeper
          </div>
          <div>
            <h1>登录工作空间</h1>
            <p>企业微信分发 · 移动端分发</p>
          </div>
          {error && <Alert type="error" title={error} showIcon />}
          <Form layout="vertical" onFinish={login} disabled={busy} className="business-form">
            <Field name="username" required label={<>账号</>}>
              <Input
                name="username"
                placeholder="请输入账号"
                autoComplete="username"
                maxLength={100}
              />
            </Field>
            <Field name="password" required label={<>密码</>}>
              <Input.Password
                name="password"
                placeholder="请输入密码"
                autoComplete="current-password"
                maxLength={128}
              />
            </Field>
            <Button type="primary" htmlType="submit" block loading={busy} disabled={busy}>
              {busy ? '正在登录…' : '登录'}
            </Button>
          </Form>
          {wecom && <a href="/api/v1/wecom/oauth/start">使用企业微信身份登录</a>}
          <p className="auth-caption">使用管理员分配的账号进入工作空间</p>
          <a href="/docs/guide/quick-start">登录与使用帮助</a>
        </section>
      </div>
    )
  return (
    <>
      {showSessionBar && (
        <div className="session-bar">
          <span>{q.data.user.display_name}</span>
          <Button onClick={logout} loading={busy} disabled={busy}>退出登录</Button>
        </div>
      )}
      {error && <Alert type="error" showIcon className="session-error" title={<>{error}</>} />}
      {children(q.data.user, logout)}
    </>
  )
}

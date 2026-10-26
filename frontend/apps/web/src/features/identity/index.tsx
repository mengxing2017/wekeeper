import { useState, type ReactNode } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Form, Input } from 'antd'
import { Field } from '../../shared/field'
import { identity, type User } from './api'
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
  async function login(values: { username: string; password: string }) {
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
  async function logout() {
    setError('')
    try {
      await identity.logout()
      await cache.cancelQueries()
      cache.clear()
    } catch (e) {
      setError((e as Error).message)
    }
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
            <p>校园社区 · 内容运营 · 企业群发</p>
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
        </section>
      </div>
    )
  return (
    <>
      {showSessionBar && (
        <div className="session-bar">
          <span>{q.data.user.display_name}</span>
          <Button onClick={logout}>退出登录</Button>
        </div>
      )}
      {error && <Alert type="error" showIcon className="session-error" title={<>{error}</>} />}
      {children(q.data.user, logout)}
    </>
  )
}

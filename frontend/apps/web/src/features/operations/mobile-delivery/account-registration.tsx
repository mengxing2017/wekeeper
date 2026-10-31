import { useId, useRef, useState, type ReactNode } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Form, Input, Select, Space, Tag } from 'antd'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { PaginationControls } from '../../../shared/pagination-controls'
import { api, type ResponsibleActor, type SendingAccount } from './api'

const actorLabel = (actor: ResponsibleActor) =>
  `${actor.display_name}（${actor.username}） · ${actor.schools.map((s) => `${s.name}管理员`).join('、')}`

export function ResponsibleActorSelect({
  value,
  onChange,
  id,
}: {
  value?: string
  onChange?: (value: string | undefined) => void
  id?: string
}) {
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [selected, setSelected] = useState<ResponsibleActor>()
  const query = useQuery({
    queryKey: ['mobile', 'platform', 'responsible-actors', search, page],
    queryFn: () => api.responsibleActors(search, page),
  })
  const actors = query.data?.items ?? []
  const options = selected && selected.id === value && !actors.some((a) => a.id === value)
    ? [selected, ...actors] : actors
  return (
    <Space orientation="vertical" className="full-width">
      <Select
        id={id}
        aria-label="本系统责任人"
        className="full-width"
        showSearch={{
          searchValue: search,
          onSearch: (text) => { setSearch(text); setPage(1) },
          filterOption: false,
        }}
        allowClear
        value={value}
        loading={query.isFetching}
        placeholder="搜索姓名或登录名，选择责任人"
        options={options.map((a) => ({ value: a.id, label: actorLabel(a) }))}
        onChange={(next) => {
          setSelected(options.find((a) => a.id === next))
          onChange?.(next)
        }}
        notFoundContent={query.isFetching ? '正在加载责任人…' : query.error ? '责任人加载失败' : search ? '未找到匹配的责任人' : '暂无已启用且具有学校管理员授权的账号'}
      />
      {query.error && <Alert type="error" showIcon title={query.error.message} action={<Button onClick={() => void query.refetch()}>重试查询</Button>} />}
      {!query.isFetching && !query.error && actors.length === 0 && (
        <span>{search ? '未找到匹配的责任人' : '暂无已启用且具有学校管理员授权的账号'}。请在账号管理中检查账号及学校管理员授权。</span>
      )}
      <a href="/admin?view=accounts">前往账号管理</a>
      <PaginationControls page={page} more={query.data?.page?.has_more} busy={query.isFetching} change={setPage} subject="责任人" />
    </Space>
  )
}

export function ResponsibleActorDisplay({ account }: { account: SendingAccount }) {
  const actor = account.responsible_actor
  if (!actor) return <span>责任人资料暂不可用</span>
  return <span>
    {actor.display_name}（{actor.username}）
    {' · '}{actor.schools.map((s) => `${s.name}管理员`).join('、') || '无有效学校管理员授权'}
    {!actor.eligible && <Tag color="error">{actor.active ? '授权已失效' : '账号已停用'}</Tag>}
  </span>
}

export function EvidenceField({ registration = false }: { registration?: boolean }) {
  const title = registration ? '登记依据' : '本次核对依据'
  return <Form.Item
    name="evidence_ref"
    label={title}
    extra={registration ? '1–500 字。说明登记来源、手机及可见企业/账号信息；保存后仍需正式核对。' : '1–500 字。说明本次在手机上核对的页面及企业、账号或完整群名，不要仅填写“已核对”。'}
    rules={[{ validator: (_, value: string | undefined) => {
      const length = [...(value ?? '').trim()].length
      return length >= 1 && length <= 500 ? Promise.resolve() : Promise.reject(new Error(`${title}需为 1–500 字，不能仅填写空白`))
    } }]}
  >
    <Input.TextArea rows={4} aria-label={title} placeholder="请记录实际来源和可核对信息" />
  </Form.Item>
}

export function ProfileForm({ children, onSave, submit = '保存' }: {
  children: ReactNode
  onSave: (values: Record<string, string>, key: string) => Promise<void>
  submit?: string
}) {
  const id = useId()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [done, setDone] = useState(false)
  const lock = useRef(false)
  const attempt = useRef<{ body: string; key: string }>(undefined)
  return <Form name={id} layout="vertical" disabled={busy} onFinish={async (values: Record<string, string>) => {
    if (lock.current) return
    lock.current = true
    const normalized = Object.fromEntries(Object.entries(values).map(([name, value]) => [name, (value ?? '').trim()]))
    const body = JSON.stringify(normalized)
    if (attempt.current?.body !== body) attempt.current = { body, key: createIdempotencyKey() }
    setBusy(true); setError(''); setDone(false)
    try {
      await onSave(normalized, attempt.current.key)
      setDone(true)
    } catch (e) {
      setError((e as { code?: string }).code === 'revision_conflict'
        ? '资料版本已变化，请刷新当前资料后重新核对。'
        : (e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }}>
    {children}
    <Button type="primary" htmlType="submit" loading={busy}>{submit}</Button>
    {error && <Alert type="error" showIcon title={error} />}
    {done && <Alert type="success" role="status" title="已保存" />}
  </Form>
}

export function AccountRegistrationForm({ saved, legacy = false }: { saved: () => Promise<void>; legacy?: boolean }) {
  return <ProfileForm onSave={async (v, key) => {
    await api.createAccount(legacy ? {
      client_kind: v.client_kind as 'wechat' | 'wecom',
      identity_namespace: v.identity_namespace,
      external_key: v.external_key,
      responsible_actor_id: v.responsible_actor_id,
      identity_evidence_ref: v.evidence_ref,
    } : {
      client_kind: 'wecom', identity_mode: 'ui_visible',
      visible_profile: { company_name: v.company_name, account_name: v.account_name, account_detail_text: v.account_detail_text || null },
      responsible_actor_id: v.responsible_actor_id,
      identity_evidence_ref: v.evidence_ref,
    }, key)
    await saved()
  }}>
    {legacy ? <>
      <Form.Item name="client_kind" label="客户端" rules={[{ required: true, message: '请选择客户端' }]}><Select options={[{ value: 'wechat', label: '微信' }, { value: 'wecom', label: '企业微信' }]} /></Form.Item>
      <Form.Item name="identity_namespace" label="身份命名空间" rules={[{ required: true, whitespace: true }]}><Input /></Form.Item>
      <Form.Item name="external_key" label="已核验的稳定身份标识" rules={[{ required: true, whitespace: true }]}><Input /></Form.Item>
    </> : <>
      <Form.Item name="company_name" label="企业微信中可见企业名称" rules={[{ required: true, whitespace: true }]}><Input /></Form.Item>
      <Form.Item name="account_name" label="企业微信中可见账号名称" rules={[{ required: true, whitespace: true }]}><Input /></Form.Item>
      <Form.Item name="account_detail_text" label="账号辅助可见文字"><Input /></Form.Item>
    </>}
    <Form.Item name="responsible_actor_id" label="本系统责任人" rules={[{ required: true, message: '请选择本系统责任人' }]} extra="责任人需要在至少一个启用学校拥有管理员授权；平台身份不代替学校授权。">
      <ResponsibleActorSelect />
    </Form.Item>
    <EvidenceField registration />
  </ProfileForm>
}

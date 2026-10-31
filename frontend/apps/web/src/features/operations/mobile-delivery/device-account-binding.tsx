import { useId, useRef, useState } from 'react'
import { Alert, Button, Checkbox, Form, Input, Select } from 'antd'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { api, type Device, type SendingAccount } from './api'

export function bindingUnavailableReason(account: SendingAccount): string | null {
  const eligibility = account.device_binding_eligibility
  if (!account.enabled || eligibility?.reason === 'account_disabled') return '账号已停用，请先启用账号。'
  if (eligibility?.reason === 'school_authorization_required') return '请先配置目标群与账号授权，并确认责任人已启用且具有对应启用学校的管理员权限。'
  if (!eligibility?.eligible) return '绑定资格暂不可用，请刷新账号资料后重试。'
  return null
}

const accountLabel = (account: SendingAccount) => account.visible_profile
  ? `${account.visible_profile.company_name} · ${account.visible_profile.account_name}`
  : `${account.client_kind} · ${account.external_key ?? account.id}`

export function DeviceAccountBinding({ device, accounts, refresh }: {
  device: Device
  accounts: SendingAccount[]
  refresh: () => Promise<unknown>
}) {
  const name = useId()
  const [form] = Form.useForm()
  const accountId = Form.useWatch('sender_account_id', form) as string | undefined
  const selected = accounts.find((a) => a.id === accountId)
  const unavailable = selected ? bindingUnavailableReason(selected) : null
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [done, setDone] = useState(false)
  const locked = useRef(false)
  const attempt = useRef<{ body: string; key: string }>(undefined)
  return <Form name={name} form={form} layout="vertical" disabled={busy} onFinish={async (values: { sender_account_id: string; note?: string }) => {
    if (locked.current || !selected || bindingUnavailableReason(selected)) return
    locked.current = true
    setBusy(true); setError(''); setDone(false)
    const input = { expected_revision: device.revision, sender_account_id: values.sender_account_id, identity_evidence_ref: (values.note ?? '').trim() }
    const body = JSON.stringify(input)
    if (attempt.current?.body !== body) attempt.current = { body, key: createIdempotencyKey() }
    try {
      await api.bindDevice(device.id, input, attempt.current.key)
      setDone(true)
      await refresh()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      locked.current = false
      setBusy(false)
    }
  }}>
    <p>绑定前请在这台手机打开业务应用的账号资料页，核对当前企业、账号名称及辅助资料。配对成功不代表业务账号一致。</p>
    <Button disabled={busy} onClick={async () => {
      if (locked.current) return
      locked.current = true
      setBusy(true); setError(''); setDone(false)
      form.setFieldValue('confirmed', false)
      try { await refresh() } catch (e) { setError((e as Error).message) } finally { locked.current = false; setBusy(false) }
    }}>刷新绑定资格</Button>
    <Form.Item name="sender_account_id" label="发送账号" rules={[{ required: true, message: '请选择发送账号' }]}>
      <Select aria-label="发送账号" placeholder="请选择发送账号" onChange={() => { form.setFieldValue('confirmed', false); setDone(false); setError('') }}
        options={accounts.map((a) => ({ value: a.id, label: accountLabel(a), disabled: Boolean(bindingUnavailableReason(a)) }))} />
    </Form.Item>
    {accounts.filter((a) => bindingUnavailableReason(a)).map((a) => <Alert key={a.id} type="warning" showIcon title={`${accountLabel(a)}：暂不可绑定`} description={bindingUnavailableReason(a)} />)}
    {accounts.length === 0 && <Alert type="info" title="暂无发送账号，请先登记发送账号，再配置目标群与账号授权。" />}
    {selected && <p>核对目标：{accountLabel(selected)}{selected.visible_profile?.account_detail_text ? ` · ${selected.visible_profile.account_detail_text}` : ''}</p>}
    {unavailable && <Alert type="warning" title={unavailable} />}
    <Form.Item name="confirmed" valuePropName="checked" rules={[{ validator: (_, value) => value === true ? Promise.resolve() : Promise.reject(new Error('请先在这台手机核对当前账号')) }]}>
      <Checkbox>已在这台手机核对当前企业和账号，与所选发送账号一致</Checkbox>
    </Form.Item>
    <Form.Item name="note" label="绑定核对备注（可选）" extra="如需补充，可记录核对页面或现场差异，最多 500 字。备注仅供人工复查，不用于设备身份识别，也不能解决授权失败。不要填写密码、验证码或设备令牌。"
      rules={[{ validator: (_, value?: string) => [...(value ?? '').trim()].length <= 500 ? Promise.resolve() : Promise.reject(new Error('绑定核对备注不能超过 500 字')) }]}>
      <Input.TextArea aria-label="绑定核对备注（可选）" rows={3} />
    </Form.Item>
    <Button type="primary" htmlType="submit" loading={busy} disabled={!selected || Boolean(unavailable)}>绑定发送账号</Button>
    {error && <Alert type="error" title={error} showIcon />}
    {done && <Alert type="success" role="status" title="绑定已保存" />}
  </Form>
}

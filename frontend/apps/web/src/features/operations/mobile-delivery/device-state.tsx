import { useId, useRef, useState } from 'react'
import { Alert, Button, Form, Input, Select, Space } from 'antd'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { api, type Device } from './api'

const conflictMessage = '设备信息已更新，已读取最新状态。请核对后再次确认保存；已填写的状态和原因已保留。'

export function DeviceStateEditor({ device, refresh }: { device: Device; refresh: () => Promise<unknown> }) {
  const [form] = Form.useForm()
  const formName = useId()
  const lock = useRef(false)
  const [busy, setBusy] = useState(false)
  const [snapshot, setSnapshot] = useState(device)
  const [revision, setRevision] = useState(device.revision)
  const [confirm, setConfirm] = useState(false)
  const [error, setError] = useState('')
  const [done, setDone] = useState(false)
  const current = device.revision > snapshot.revision ? device : snapshot

  async function readLatest() {
    const latest = await api.device(device.id)
    setSnapshot(latest)
    return latest
  }
  async function reload() {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setDone(false)
    try {
      const latest = await readLatest()
      setConfirm(latest.revision !== revision || confirm)
      setRevision(latest.revision)
      setError('')
      await refresh()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  async function save(values: { enabled: string; reason: string }) {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    setDone(false)
    try {
      // Reading the current record catches pairing changes; the PATCH still checks
      // its revision so a later concurrent change can never be silently overwritten.
      const latest = await readLatest()
      if (latest.revision !== revision) {
        setRevision(latest.revision)
        setConfirm(true)
        setError(conflictMessage)
        await refresh()
        return
      }
      const updated = await api.updateDevice(device.id, {
        expected_revision: latest.revision,
        enabled: values.enabled === 'true',
        reason: values.reason.trim(),
      }, createIdempotencyKey())
      setSnapshot(updated)
      setRevision(updated.revision)
      setConfirm(false)
      setDone(true)
      await refresh()
    } catch (e) {
      if ((e as { code?: string }).code === 'revision_conflict') {
        setConfirm(true)
        try {
          const latest = await readLatest()
          setRevision(latest.revision)
          setError(conflictMessage)
          await refresh()
        } catch {
          setError('设备信息已更新，但最新状态读取失败。请先刷新设备状态，核对后重新提交；已填写内容已保留。')
        }
      } else {
        setError((e as Error).message)
      }
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  return <Form form={form} name={formName} layout="vertical" className="business-form"
    disabled={busy} initialValues={{ enabled: String(device.enabled), reason: '' }} onFinish={save}>
    <p>当前设备状态：{current.enabled ? '已启用' : '已停用'}。设备启用后仍需业务授权和任务确认才可执行。</p>
    {(confirm || current.revision !== revision) && <Alert type="warning" showIcon
      title="请核对最新设备状态后确认保存" description="其他管理操作或手机配对可能已更新设备信息。输入内容已保留，不会自动覆盖最新记录。" />}
    <Form.Item name="enabled" label="设备状态" rules={[{ required: true, message: '请选择设备状态' }]}>
      <Select options={[{ value: 'true', label: '启用' }, { value: 'false', label: '停用' }]} />
    </Form.Item>
    <Form.Item name="reason" label="变更原因" rules={[{ required: true, whitespace: true, max: 1000, message: '请填写 1～1000 字的变更原因' }]}>
      <Input maxLength={1000} />
    </Form.Item>
    <Space wrap>
      <Button type="primary" htmlType="submit" loading={busy}>{confirm ? '确认并保存设备状态' : '保存设备状态'}</Button>
      <Button disabled={busy} onClick={() => void reload()}>刷新设备状态</Button>
    </Space>
    {error && <Alert type="error" title={error} showIcon />}
    {done && <Alert type="success" role="status" title="设备状态已保存" showIcon />}
  </Form>
}

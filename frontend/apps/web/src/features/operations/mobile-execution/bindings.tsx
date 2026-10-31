import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Modal, Select, Space, Table } from 'antd'
import { api, type TargetBinding } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'

export function MobileTargetBindings({ school, admin }: { school: string; admin: boolean }) {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [open, setOpen] = useState(false)
  const [targetPage, setTargetPage] = useState(1)
  const [devicePage, setDevicePage] = useState(1)
  const [form] = Form.useForm<{ target_id: string; device_ids: string[] }>()
  const rows = useQuery({ queryKey: ['mobile-execution', school, 'bindings', page], queryFn: () => api.bindings(school, page) })
  const targets = useQuery({ queryKey: ['mobile-execution', school, 'targets', targetPage, ''], queryFn: () => api.targets(school, targetPage), enabled: open })
  const devices = useQuery({ queryKey: ['mobile-execution', school, 'devices', devicePage], queryFn: () => api.devices(school, devicePage), enabled: open })
  const invalidate = () => client.invalidateQueries({ queryKey: ['mobile-execution', school] })
  const bind = useMutation({ mutationFn: (v: { target_id: string; device_ids: string[] }) => api.bind(school, v.target_id, v.device_ids, createIdempotencyKey()), onSuccess: () => { setOpen(false); form.resetFields(); void invalidate() } })
  const unbind = useMutation({ mutationFn: (row: TargetBinding) => api.unbind(school, row.id, row.revision, '管理员解除设备目标绑定', createIdempotencyKey()), onSuccess: invalidate })
  return <Card title="设备与手机目标绑定" extra={admin && <Button onClick={() => setOpen(true)}>绑定设备</Button>}>
    <p>绑定表示允许该设备选择此目标，不代表手机已加入群或平台已验证真实群身份。解除绑定不删除目标，也不影响其他设备。</p>
    {(rows.error || bind.error || unbind.error) && <Alert type="error" title={(rows.error || bind.error || unbind.error)?.message} />}
    <Table<TargetBinding> rowKey="id" dataSource={rows.data?.items ?? []} loading={rows.isFetching} pagination={false} columns={[
      { title: '设备', render: (_, row) => row.device_name || row.device_id }, { title: '下发群名', render: (_, row) => row.search_text || row.target_id },
      { title: '平台别名', dataIndex: 'target_alias' }, { title: '可用状态', render: (_, row) => row.available ? '资源有效，执行仍需授权及确认' : '请检查学校设备授权、设备或目标启停' },
      { title: '绑定版本', dataIndex: 'revision' },
      ...(admin ? [{ title: '操作', render: (_: unknown, row: TargetBinding) => <Button danger onClick={() => Modal.confirm({ title: '解除此设备与目标的绑定？', onOk: () => unbind.mutateAsync(row) })}>解绑</Button> }] : []),
    ]} />
    <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="设备目标绑定" />
    <Modal title="绑定本校授权设备" open={open} confirmLoading={bind.isPending} onCancel={() => setOpen(false)} onOk={() => void form.validateFields().then(v => bind.mutate(v))}>
      <Form form={form} layout="vertical">
        <Form.Item name="target_id" label="手机目标群" rules={[{ required: true }]}><Select options={targets.data?.items.filter(t => t.enabled).map(t => ({ value: t.id, label: t.alias ? `${t.alias}（${t.search_text}）` : t.search_text }))} /></Form.Item>
        <PaginationControls page={targetPage} more={targets.data?.page?.has_more} change={setTargetPage} subject="可选目标群" />
        <Form.Item name="device_ids" label="设备（可多选）" rules={[{ required: true, type: 'array', min: 1 }]}><Select mode="multiple" options={devices.data?.items.filter(d => d.enabled).map(d => ({ value: d.id, label: d.school_alias || d.name }))} /></Form.Item>
        <PaginationControls page={devicePage} more={devices.data?.page?.has_more} change={setDevicePage} subject="可选设备" />
        <Space direction="vertical">{targets.error && <Alert type="error" title={targets.error.message} />}{devices.error && <Alert type="error" title={devices.error.message} />}</Space>
      </Form>
    </Modal>
  </Card>
}

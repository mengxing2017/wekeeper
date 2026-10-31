import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Input, Modal, Select, Table, Tag } from 'antd'
import { api, type DeviceGrant } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

export function MobileDeviceGrants({ schools }: { schools: Array<{ id: string; name: string }> }) {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [devicePage, setDevicePage] = useState(1)
  const [open, setOpen] = useState(false)
  const [form] = Form.useForm<{ school_id: string; device_id: string; purpose: string }>()
  const grants = useQuery({ queryKey: ['mobile-execution', 'platform', 'grants', page], queryFn: () => api.grants(page) })
  const devices = useQuery({ queryKey: ['mobile-execution', 'platform', 'devices', devicePage], queryFn: () => api.platformDevices(devicePage), enabled: open })
  const invalidate = () => client.invalidateQueries({ queryKey: ['mobile-execution'] })
  const grant = useMutation({ mutationFn: (v: { school_id: string; device_id: string; purpose: string }) => api.grant(v, createIdempotencyKey()), onSuccess: () => { setOpen(false); form.resetFields(); void invalidate() } })
  const revoke = useMutation({ mutationFn: (v: DeviceGrant) => api.revokeGrant(v.id, v.revision, '平台管理员撤销学校设备使用权', createIdempotencyKey()), onSuccess: invalidate })
  return <Card title="学校设备使用授权" extra={<Button type="primary" onClick={() => setOpen(true)}>授予使用权</Button>}>
    <Alert type="info" title="学校使用权独立于设备保管人和发送账号" description="授予后还需学校配置流程、手机现场接受授权及人工确认任务；这里不能代替手机接受。撤销手机授权不影响原有企业微信客户群和群发。" />
    {(grants.error || grant.error || revoke.error) && <Alert type="error" title={(grants.error || grant.error || revoke.error)?.message} />}
    <Table<DeviceGrant> rowKey="id" dataSource={grants.data?.items ?? []} loading={grants.isFetching} pagination={false} columns={[
      { title: '学校', render: (_, row) => row.school_name || schools.find(s => s.id === row.school_id)?.name || row.school_id },
      { title: '设备', render: (_, row) => row.device_name || row.device_id }, { title: '用途', dataIndex: 'purpose' },
      { title: '状态', render: (_, row) => <Tag>{row.enabled ? '已授予' : '已撤销'}</Tag> },
      { title: '版本', dataIndex: 'revision' }, { title: '操作', render: (_, row) => row.enabled && <Button danger onClick={() => Modal.confirm({ title: '撤销学校设备使用权？', content: '撤销后该学校不能再使用此设备领取新执行许可。', onOk: () => revoke.mutateAsync(row) })}>撤销</Button> },
    ]} />
    <PaginationControls page={page} more={grants.data?.page?.has_more} change={setPage} subject="学校设备授权" />
    <Modal title="授予学校设备使用权" open={open} confirmLoading={grant.isPending} onCancel={() => setOpen(false)} onOk={() => void form.validateFields().then(v => grant.mutate(v))}>
      <Form form={form} layout="vertical">
        <Form.Item name="school_id" label="学校" rules={[{ required: true }]}><Select options={schools.map(s => ({ value: s.id, label: s.name }))} /></Form.Item>
        <Form.Item name="device_id" label="手机设备" rules={[{ required: true }]}><Select options={devices.data?.items.map(d => ({ value: d.id, label: d.name }))} /></Form.Item>
        <PaginationControls page={devicePage} more={devices.data?.page?.has_more} change={setDevicePage} subject="待授权设备" />
        <Form.Item name="purpose" label="明确授权用途" rules={[{ required: true }]}><Input /></Form.Item>
        {devices.error && <Alert type="error" title={devices.error.message} />}
      </Form>
    </Modal>
  </Card>
}

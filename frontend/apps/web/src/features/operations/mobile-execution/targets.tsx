import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { MobileTargetBindings } from './bindings'
import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Input, Modal, Select, Space, Switch, Table, Tag } from 'antd'
import { api, type MobileTarget, type TargetInput } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'

export function MobileTargets({ school, admin }: { school: string; admin: boolean }) {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [devicePage, setDevicePage] = useState(1)
  const [search, setSearch] = useState('')
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<MobileTarget>()
  const [editingEnabled, setEditingEnabled] = useState(true)
  const [pendingDuplicate, setPendingDuplicate] = useState<{ input: TargetInput; token: string }>()
  const [form] = Form.useForm<TargetInput>()
  const targets = useQuery({ queryKey: ['mobile-execution', school, 'targets', page, search], queryFn: () => api.targets(school, page, search) })
  const devices = useQuery({ queryKey: ['mobile-execution', school, 'devices', devicePage], queryFn: () => api.devices(school, devicePage), enabled: admin })
  const invalidate = () => client.invalidateQueries({ queryKey: ['mobile-execution', school] })
  const save = useMutation({ mutationFn: async (input: TargetInput) => {
    if (editing) return api.updateTarget(school, editing.id, { search_text: input.search_text, alias: input.alias ?? '', remark: input.remark ?? '', enabled: editingEnabled, expected_revision: editing.metadata_revision }, createIdempotencyKey())
    return api.createTarget(school, input, createIdempotencyKey())
  }, onSuccess: (result, input) => {
    if ('confirmation_required' in result) { setPendingDuplicate({ input, token: result.duplicate_confirmation }); return }
    setPendingDuplicate(undefined); setOpen(false); form.resetFields(); void invalidate()
  } })
  const archive = useMutation({ mutationFn: (row: MobileTarget) => api.archiveTarget(school, row.id, row.metadata_revision, '管理员归档目标', createIdempotencyKey()), onSuccess: invalidate })
  return <><Card title="手机目标群" extra={admin && <Button type="primary" onClick={() => { setEditing(undefined); form.resetFields(); setOpen(true) }}>新增目标群</Button>}>
    <Alert type="info" showIcon title="手机目标群独立于官方客户群" description="下发群名是手机搜索文字，可填写手机内的群名备注。设备只发送群分类的第一个搜索结果，请现场核实。此处不验证真实群身份。" />
    <Input.Search aria-label="搜索手机目标群" placeholder="搜索下发群名或别名" allowClear onSearch={value => { setSearch(value); setPage(1) }} />
    {(targets.error || save.error || archive.error) && <Alert type="error" title={(targets.error || save.error || archive.error)?.message} />}
    <Table<MobileTarget> rowKey="id" loading={targets.isFetching} dataSource={targets.data?.items ?? []} pagination={false} locale={{ emptyText: '暂无手机目标群。管理员可新增目标，再绑定已获授权的设备。' }} columns={[
      { title: '下发群名', dataIndex: 'search_text' }, { title: '平台别名', dataIndex: 'alias' }, { title: '管理备注', dataIndex: 'remark' },
      { title: '状态', render: (_, row) => <Tag>{row.enabled ? '启用' : '停用'}</Tag> },
      ...(admin ? [{ title: '操作', render: (_: unknown, row: MobileTarget) => <Space><Button onClick={() => { setEditing(row); setEditingEnabled(row.enabled); form.setFieldsValue(row); setOpen(true) }}>编辑</Button><Button danger onClick={() => Modal.confirm({ title: '归档此手机目标群？', content: '归档后不再作为新任务候选，不影响官方客户群。', onOk: () => archive.mutateAsync(row) })}>归档</Button></Space> }] : []),
    ]} />
    <PaginationControls page={page} more={targets.data?.page?.has_more} change={setPage} subject="手机目标群" />
    <Modal title={editing ? '编辑手机目标群' : '新增手机目标群'} open={open} confirmLoading={save.isPending} onCancel={() => { setOpen(false); setPendingDuplicate(undefined) }} onOk={() => void form.validateFields().then(v => save.mutate(v))}>
      <Form form={form} layout="vertical">
        <Form.Item name="search_text" label="下发群名" rules={[{ required: true, message: '请填写手机内实际使用的群搜索文字' }]}><Input /></Form.Item>
        <Form.Item name="alias" label="平台别名（不用于搜索）"><Input /></Form.Item>
        {editing && <Form.Item label="目标状态"><Switch checked={editingEnabled} onChange={setEditingEnabled} checkedChildren="启用" unCheckedChildren="停用" /></Form.Item>}
        <Form.Item name="remark" label="管理备注（不用于搜索）"><Input.TextArea /></Form.Item>
        {!editing && <Form.Item name="device_ids" label="同时绑定本校授权设备"><Select mode="multiple" options={devices.data?.items.filter(d => d.enabled).map(d => ({ value: d.id, label: d.school_alias || d.name }))} /></Form.Item>}
        {!editing && <PaginationControls page={devicePage} more={devices.data?.page?.has_more} change={setDevicePage} subject="可绑定设备" />}
        {devices.error && <Alert type="error" title={devices.error.message} />}
      </Form>
    </Modal>
    <Modal title="存在相同搜索文字的目标" open={!!pendingDuplicate} confirmLoading={save.isPending} okText="仍作为独立目标新增" onCancel={() => setPendingDuplicate(undefined)} onOk={() => pendingDuplicate && save.mutate({ ...pendingDuplicate.input, duplicate_confirmation: pendingDuplicate.token })}>
      相同搜索文字不代表同一真实群。确认后将新增独立记录。
    </Modal>
  </Card><MobileTargetBindings school={school} admin={admin} /></>
}

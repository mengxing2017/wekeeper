import {BindingBudgets} from './binding-budgets'
import { TableRowActions } from '../../../shared/table-row-actions'
import { TableText } from '../../../shared/table-text'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Input, Modal, Space, Switch, Table, Tag } from 'antd'
import { api, type MobileTarget, type TargetInput } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'

export function MobileTargets({  admin }: { admin: boolean }) {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [search, setSearch] = useState('')
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<MobileTarget>()
  const [editingEnabled, setEditingEnabled] = useState(true)
  const [pendingDuplicate, setPendingDuplicate] = useState<{ input: TargetInput; token: string }>()
  const [form] = Form.useForm<TargetInput>()
  const targets = useQuery({ queryKey: ['mobile-execution', 'targets', page, search], queryFn: () => api.targets(page, search) })
  const invalidate = () => client.invalidateQueries({ queryKey: ['mobile-execution'] })
  const save = useMutation({ mutationFn: async (input: TargetInput) => {
    if (editing) return api.updateTarget(editing.id, { search_text: input.search_text, remark: input.remark ?? '', enabled: editingEnabled, expected_revision: editing.metadata_revision }, createIdempotencyKey())
    return api.createTarget(input, createIdempotencyKey())
  }, onSuccess: (result, input) => {
    if ('confirmation_required' in result) { setPendingDuplicate({ input, token: result.duplicate_confirmation }); return }
    setPendingDuplicate(undefined); setOpen(false); form.resetFields(); void invalidate()
  } })
  const archive = useMutation({ mutationFn: (row: MobileTarget) => api.archiveTarget(row.id, row.metadata_revision, '管理员归档目标', createIdempotencyKey()), onSuccess: invalidate })
  return <Card title="手机目标群" extra={admin && <Space wrap><Button href="/admin?view=mobile-business-schemes">前往默认手机群消息业务方案</Button><Button type="primary" onClick={() => { setEditing(undefined); form.resetFields(); setOpen(true) }}>新增</Button></Space>}>
    <Alert type="info" showIcon title="手机目标群独立于官方客户群" description="填写一个群名称即可。手机自行搜索并识别目标，识别失败只影响本次设备尝试。" />
    <Input.Search aria-label="搜索手机目标群" placeholder="搜索群名称" allowClear onSearch={value => { setSearch(value); setPage(1) }} />
    {(targets.error || save.error || archive.error) && <Alert type="error" title={(targets.error || save.error || archive.error)?.message} />}
    <Table<MobileTarget> tableLayout="fixed" scroll={{x:620}} rowKey="id" loading={targets.isFetching} dataSource={targets.data?.items ?? []} pagination={false} locale={{ emptyText: '暂无手机目标群。管理员可新增目标，再联系平台资源维护人员绑定共享手机。' }} columns={[
      { title: '群名称', width:220, dataIndex:'search_text', render:value=><TableText value={value}/> }, { title: '备注', dataIndex:'remark', render:value=><TableText value={value}/> },
      { title: '状态', width:80, render: (_, row) => <Tag>{row.enabled ? '启用' : '停用'}</Tag> },
      ...(admin ? [{ title: '操作', width:128, render: (_: unknown, row: MobileTarget) => <TableRowActions rowLabel={row.search_text} actions={[
        {key:'edit',label:'编辑',onClick:()=>{setEditing(row);setEditingEnabled(row.enabled);form.setFieldsValue(row);setOpen(true)}},
        {key:'archive',label:'归档',danger:true,confirm:{title:'归档此手机目标群？',description:'归档后不再作为新任务候选，不影响官方客户群。'},onClick:()=>archive.mutateAsync(row)},
      ]}/> }] : []),
    ]} />
    <PaginationControls page={page} more={targets.data?.page?.has_more} change={setPage} subject="手机目标群" />
    {admin&&<BindingBudgets/>}
    <Modal title={editing ? '编辑手机目标群' : '新增手机目标群'} open={open} confirmLoading={save.isPending} onCancel={() => { setOpen(false); setPendingDuplicate(undefined) }} onOk={() => void form.validateFields().then(v => save.mutate(v))}>
      <Form form={form} layout="vertical">
        <Form.Item name="search_text" label="群名称" rules={[{ required: true, message: '请填写群名称' }]}><Input /></Form.Item>
        {editing && <Form.Item label="状态"><Switch aria-label="目标状态" checked={editingEnabled} onChange={setEditingEnabled} checkedChildren="启用" unCheckedChildren="停用" /></Form.Item>}
        <Form.Item name="remark" label="备注" extra="不用于搜索"><Input.TextArea aria-label="管理备注（不用于搜索）" /></Form.Item>
      </Form>
    </Modal>
    <Modal title="存在相同群名称的目标" open={!!pendingDuplicate} confirmLoading={save.isPending} okText="仍作为独立目标新增" onCancel={() => setPendingDuplicate(undefined)} onOk={() => pendingDuplicate && save.mutate({ ...pendingDuplicate.input, duplicate_confirmation: pendingDuplicate.token })}>
      相同群名称不代表同一真实群。确认后将新增独立记录。
    </Modal>
  </Card>
}

import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Input, Modal, Select, Space, Table, Tag, Tabs } from 'antd'
import { api, type SharedBindingInput, type TargetBinding } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

import { DeviceCapabilities } from './capabilities'

export function SharedMobileResources() {
  return <Tabs items={[{ key: 'bindings', label: '手机与目标群绑定', children: <SharedBindings /> }, { key: 'capabilities', label: '业务能力开通', children: <DeviceCapabilities /> }]} />
}

function SharedBindings() {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [search, setSearch] = useState('')
  const [devicePage, setDevicePage] = useState(1)
  const [targetPage, setTargetPage] = useState(1)
  const [targetSearch, setTargetSearch] = useState('')
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<TargetBinding>()
  const [form] = Form.useForm<SharedBindingInput>()
  const rows = useQuery({ queryKey: ['mobile-execution', 'platform', 'bindings', page, search], queryFn: () => api.platformBindings(page, search) })
  const devices = useQuery({ queryKey: ['mobile-execution', 'platform', 'devices', devicePage], queryFn: () => api.platformDevices(devicePage), enabled: open && !editing })
  const targets = useQuery({ queryKey: ['mobile-execution', 'platform', 'targets', targetPage, targetSearch], queryFn: () => api.platformTargets(targetPage, targetSearch), enabled: open && !editing })
  const invalidate = () => client.invalidateQueries({ queryKey: ['mobile-execution'] })
  const save = useMutation({ mutationFn: async (v: SharedBindingInput): Promise<void> => {
    const override = v.search_text_override === '' || v.search_text_override == null ? null : v.search_text_override
    if (editing) await api.updateBinding(editing.id, { expected_revision: editing.revision, enabled: true, search_text_override: override }, createIdempotencyKey())
    else await api.bind({ target_id: v.target_id, device_ids: v.device_ids, search_text_override: override }, createIdempotencyKey())
  }, onSuccess: () => { setOpen(false); setEditing(undefined); form.resetFields(); void invalidate() } })
  const revoke = useMutation({ mutationFn: (row: TargetBinding) => api.unbind(row.id, row.revision, '平台资源维护人员停用目标绑定', createIdempotencyKey()), onSuccess: invalidate })
  return <Card title="共享手机与目标群" extra={<Button type="primary" onClick={() => { setEditing(undefined); form.resetFields(); setOpen(true) }}>绑定共享手机</Button>}>
    <Alert type="info" showIcon title="通过目标群配置执行范围，不再授予整所学校设备使用权" description="一台手机可服务多所学校，一个群可有多台候选手机。绑定不是群成员身份核验；学校只能查看本校绑定。旧关系须人工核实后重新启用。" />
    <Input.Search aria-label="搜索平台手机群绑定" placeholder="搜索手机、目标群默认名或别名" allowClear onSearch={v => { setSearch(v); setPage(1) }} />
    {(rows.error || save.error || revoke.error) && <Alert type="error" title={(rows.error || save.error || revoke.error)?.message} />}
    <Table<TargetBinding> rowKey="id" loading={rows.isFetching} dataSource={rows.data?.items ?? []} pagination={false} columns={[
      { title: '学校', render: (_, r) => r.school_name || r.school_id }, { title: '目标群', render: (_, r) => r.target_alias || r.target_id },
      { title: '手机', render: (_, r) => r.device_name || r.device_id },
      { title: '有效搜索文字', render: (_, r) => <span style={{ whiteSpace: 'pre-wrap' }}>{r.search_text}</span> },
      { title: '来源', render: (_, r) => r.search_source === 'binding_override' ? '该手机覆盖名' : '群默认名' },
      { title: '状态', render: (_, r) => <Tag>{r.enabled ? '已绑定' : '已停用'}</Tag> },
      { title: '操作', render: (_, r) => <Space><Button onClick={() => { setEditing(r); form.setFieldsValue({ target_id: r.target_id, device_ids: [r.device_id], search_text_override: r.search_text_override }); setOpen(true) }}>{r.enabled ? '修改搜索名' : '核实并启用'}</Button>{r.enabled && <Button danger onClick={() => Modal.confirm({ title: '停用此手机与群的绑定？', content: '仅影响这一绑定，不停用手机或其他学校的群。历史记录保留。', onOk: () => revoke.mutateAsync(r) })}>停用绑定</Button>}</Space> },
    ]} />
    <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="共享手机绑定" />
    <Modal title={editing ? '核实手机群绑定与搜索名' : '为目标群批量绑定手机'} open={open} confirmLoading={save.isPending} onCancel={() => setOpen(false)} onOk={() => void form.validateFields().then(v => save.mutate(v))}>
      {editing && <p>手机：{editing.device_name || editing.device_id}；目标：{editing.target_alias || editing.target_id}。请核实实际可搜索到的群。</p>}
      <Form form={form} layout="vertical">
        {!editing && <><Input.Search aria-label="搜索可绑定目标" placeholder="按学校、群名或别名查找" onSearch={v => { setTargetSearch(v); setTargetPage(1) }} />
          <Form.Item name="target_id" label="学校目标群" rules={[{ required: true }]}><Select options={targets.data?.items.filter(t => t.enabled).map(t => ({ value: t.id, label: `${t.school_name} / ${t.alias || t.search_text}` }))} /></Form.Item>
          <PaginationControls page={targetPage} more={targets.data?.page?.has_more} change={setTargetPage} subject="可绑定目标群" />
          <Form.Item name="device_ids" label="共享手机（可多选）" rules={[{ required: true, type: 'array', min: 1 }]}><Select mode="multiple" options={devices.data?.items.filter(d => d.enabled).map(d => ({ value: d.id, label: d.name }))} /></Form.Item>
          <PaginationControls page={devicePage} more={devices.data?.page?.has_more} change={setDevicePage} subject="共享手机" /></>}
        <Form.Item name="search_text_override" label="该手机下发群名（留空继承群默认名）"><Input maxLength={256} /></Form.Item>
        <p>原样保留搜索文字；修改后相关未调用计划须重新预览确认。批量绑定的手机使用相同覆盖名，需不同名称时分别配置。</p>
        {(targets.error || devices.error) && <Alert type="error" title={(targets.error || devices.error)?.message} />}
      </Form>
    </Modal>
  </Card>
}

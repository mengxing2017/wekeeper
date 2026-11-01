import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Input, Modal, Select, Space, Switch, Table, Tag } from 'antd'
import { api, type DeviceCapabilityConfiguration, type DeviceCapabilityConfigurationInput, type SharedFlowV2 } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

type Publication = { id?: string; input: DeviceCapabilityConfigurationInput; key: string }

export function DeviceCapabilities() {
  const client = useQueryClient()
  const importGeneration = useRef(0)
  const [page, setPage] = useState(1)
  const [devicePage, setDevicePage] = useState(1)
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<DeviceCapabilityConfiguration>()
  const [flow, setFlow] = useState<SharedFlowV2>()
  const [importError, setImportError] = useState('')
  const [pending, setPending] = useState<Publication>()
  const [form] = Form.useForm<{ device_id: string; name: string; enabled: boolean }>()
  const rows = useQuery({ queryKey: ['mobile-execution', 'platform', 'capabilities', page], queryFn: () => api.platformCapabilities(page) })
  const devices = useQuery({ queryKey: ['mobile-execution', 'platform', 'devices', devicePage], queryFn: () => api.platformDevices(devicePage), enabled: open && !editing })
  const save = useMutation({ mutationFn: (request: Publication) => request.id ? api.updateCapability(request.id, request.input, request.key) : api.createCapability(request.input, request.key), onSuccess: () => {
    setPending(undefined); setOpen(false); void client.invalidateQueries({ queryKey: ['mobile-execution'] })
  } })
  function edit(row?: DeviceCapabilityConfiguration) {
    importGeneration.current += 1
    setEditing(row); setFlow(row?.flow); setImportError(''); save.reset()
    form.setFieldsValue({ device_id: row?.device_id, name: row?.name ?? '', enabled: row?.enabled ?? false }); setOpen(true)
  }
  async function importFlow(file?: File) {
    if (!file) return
    const generation = ++importGeneration.current
    setFlow(undefined)
    try {
      if (file.size > 262144) throw new Error('配置文件不能超过 256 KiB')
      const contents = await file.text()
      if (generation !== importGeneration.current) return
      const value: unknown = JSON.parse(contents)
      if (!value || typeof value !== 'object' || !('protocol_version' in value) || value.protocol_version !== 7 || !('schema_version' in value) || value.schema_version !== 2 || !('engine_version' in value) || value.engine_version !== 'otter-ui/2' || !('app_key' in value) || value.app_key !== 'wecom' || !('capability' in value) || value.capability !== 'group_text' || !('selection_policy' in value) || value.selection_policy !== 'first_group_result' || !('package_name' in value) || typeof value.package_name !== 'string' || !('application_label' in value) || typeof value.application_label !== 'string') throw new Error('请选择 v7 企业微信群文字配置文件；旧学校配置不能直接导入')
      setFlow(value as SharedFlowV2); setImportError('')
    } catch (error) { if (generation !== importGeneration.current) return; setFlow(undefined); setImportError(error instanceof Error ? error.message : '读取配置失败') }
  }
  async function prepare() {
    const values = await form.validateFields()
    if (!flow) { setImportError('请先导入经过现场核对的 v7 配置文件'); return }
    const request: Publication = { id: editing?.id, input: { ...values, expected_revision: editing?.revision ?? 0, flow }, key: createIdempotencyKey() }
    Modal.confirm({ title: '发布这份手机全局配置？', content: '发布后产生新版本，手机须重新确认同意。启用配置不会开放实际发送。配置文件的完整结构和执行预算由平台再次校验。', onOk: () => { setPending(request); save.mutate(request) } })
  }
  return <Card title="设备全局能力配置" extra={<Button type="primary" disabled={!!pending} onClick={() => edit()}>新增设备配置</Button>}>
    <Alert type="warning" showIcon title="配置启用不代表设备可以发送" description="配置属于共享手机，不属于学校。手机需要在本机确认当前版本；现场验证、租约和发送许可尚未完成，实际发送保持关闭。本页不推断手机本地同意或执行状态。" />
    {rows.error && <Alert type="error" title={rows.error.message} />}
    <Table<DeviceCapabilityConfiguration> rowKey="id" loading={rows.isFetching} dataSource={rows.data?.items ?? []} pagination={false} columns={[
      { title: '配置名称', dataIndex: 'name' }, { title: '设备编号', dataIndex: 'device_id' }, { title: '版本', dataIndex: 'revision' },
      { title: '配置状态', render: (_, row) => <Tag>{row.enabled ? '已启用，须手机确认' : '已停用'}</Tag> },
      { title: '配置摘要', render: (_, row) => <span style={{ overflowWrap: 'anywhere' }}>{row.configuration_digest}</span> },
      { title: '操作', render: (_, row) => <Button disabled={!!pending} onClick={() => edit(row)}>查看与修改</Button> },
    ]} />
    <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="设备能力配置" />
    <Modal title={editing ? '查看与修改设备配置' : '新增设备配置'} open={open} footer={null} closable={!pending} mask={{ closable: !pending }} onCancel={() => setOpen(false)}>
      <Form form={form} layout="vertical" disabled={!!pending}>
        <Form.Item name="device_id" label="共享手机" rules={[{ required: true }]}>{editing ? <Input disabled /> : <Select options={devices.data?.items.filter(d => d.enabled).map(d => ({ value: d.id, label: d.name }))} />}</Form.Item>
        {!editing && <PaginationControls page={devicePage} more={devices.data?.page?.has_more} change={setDevicePage} subject="配置可用手机" />}
        <Form.Item name="name" label="配置名称" rules={[{ required: true, whitespace: true, max: 128 }]}><Input maxLength={128} /></Form.Item>
        <Form.Item name="enabled" label="启用这份配置" valuePropName="checked"><Switch /></Form.Item>
      </Form>
      <p>导入维护人员针对手机实际应用界面核对的 v7 JSON 配置文件；不会转换旧学校配置或使用示例规则替代现场核对。</p>
      <input aria-label="导入 v7 设备能力配置" type="file" accept=".json,application/json" disabled={!!pending} onChange={event => { void importFlow(event.target.files?.[0]); event.target.value = '' }} />
      {flow && <p>应用：{flow.application_label}；安装包：{flow.package_name}；能力：群文字；选择规则：仅首个群结果。</p>}
      {(importError || devices.error) && <Alert type="error" title={importError || devices.error?.message} />}
      {save.error && <Alert type="error" title="发布结果尚未确认" description={`${save.error.message}。请使用下方按钮重试同一请求；重试前请勿重复创建配置。若为版本冲突或校验错误，可关闭并重新载入，但原请求仍可能已被平台接受。`} />}
      <Space style={{ marginTop: 16 }}>
        {!pending && <Button type="primary" onClick={() => { void prepare().catch(() => undefined) }}>核对并发布</Button>}
        {pending && <><Button type="primary" loading={save.isPending} onClick={() => save.mutate(pending)}>重试同一发布请求</Button><Button disabled={save.isPending} onClick={() => { setPending(undefined); setOpen(false); void client.invalidateQueries({ queryKey: ['mobile-execution'] }) }}>关闭并重新载入列表</Button></>}
      </Space>
    </Modal>
  </Card>
}

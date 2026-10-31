import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Input, Modal, Select, Space, Table, Tag } from 'antd'
import { api, type FlowV2, type FlowValidation, type FlowRelease, type ExecutionAssignment } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

const validationLabel = (v: FlowValidation) => v.state === 'passed' ? '设备报告验证通过' : v.state === 'failed' ? '设备报告验证失败' : v.expired ? '已过期，请重新验证' : '等待手机验证'
export function MobileExecutionFlows({ school, admin }: { school: string; admin: boolean }) {
  const client = useQueryClient()
  const [flowPage, setFlowPage] = useState(1)
  const [validationPage, setValidationPage] = useState(1)
  const [releasePage, setReleasePage] = useState(1)
  const [assignmentPage, setAssignmentPage] = useState(1)
  const [devicePage, setDevicePage] = useState(1)
  const [registerOpen, setRegisterOpen] = useState(false)
  const [validateOpen, setValidateOpen] = useState(false)
  const [assignOpen, setAssignOpen] = useState(false)
  const [editing, setEditing] = useState<ExecutionAssignment>()
  const [registerForm] = Form.useForm<{ name: string; document: string }>()
  const [validationForm] = Form.useForm<{ device_id: string; flow_version_id: string; search_text: string }>()
  const [assignmentForm] = Form.useForm<{ device_id: string; release_id: string }>()
  const flows = useQuery({ queryKey: ['mobile-execution', school, 'flows', flowPage], queryFn: () => api.flows(school, flowPage) })
  const validations = useQuery({ queryKey: ['mobile-execution', school, 'validations', validationPage], queryFn: () => api.validations(school, validationPage) })
  const releases = useQuery({ queryKey: ['mobile-execution', school, 'releases', releasePage], queryFn: () => api.releases(school, releasePage) })
  const assignments = useQuery({ queryKey: ['mobile-execution', school, 'assignments', assignmentPage], queryFn: () => api.assignments(school, assignmentPage) })
  const devices = useQuery({ queryKey: ['mobile-execution', school, 'devices', devicePage], queryFn: () => api.devices(school, devicePage), enabled: admin })
  const refresh = () => client.invalidateQueries({ queryKey: ['mobile-execution', school] })
  const register = useMutation({ mutationFn: (v: { name: string; document: string }) => {
    let flow: FlowV2
    try { flow = JSON.parse(v.document) as FlowV2 } catch { throw new Error('流程文件不是有效 JSON，请联系流程维护人员提供完整文件') }
    return api.createFlow(school, { name: v.name, flow }, createIdempotencyKey())
  }, onSuccess: () => { setRegisterOpen(false); registerForm.resetFields(); void refresh() } })
  const validate = useMutation({ mutationFn: (v: { device_id: string; flow_version_id: string; search_text: string }) => api.validateFlow(school, v, createIdempotencyKey()), onSuccess: () => { setValidateOpen(false); validationForm.resetFields(); void refresh() } })
  const release = useMutation({ mutationFn: (v: FlowValidation) => api.releaseFlow(school, { flow_version_id: v.flow_version_id, validation_id: v.id, name: `${[...(v.flow_name || '手机流程')].slice(0,100).join('')}（设备验证发布）` }, createIdempotencyKey()), onSuccess: refresh })
  const revoke = useMutation({ mutationFn: (v: FlowRelease) => api.revokeRelease(school, v.id, v.revision, '管理员撤销流程发布', createIdempotencyKey()), onSuccess: refresh })
  const assign = useMutation({ mutationFn: (v: { device_id: string; release_id: string }) => {
    const input = { ...v, enabled: true, expected_revision: editing?.execution_revision ?? 0 }
    return editing ? api.updateAssignment(school, editing.id, input, createIdempotencyKey()) : api.assign(school, input, createIdempotencyKey())
  }, onSuccess: () => { setAssignOpen(false); assignmentForm.resetFields(); void refresh() } })
  const disable = useMutation({ mutationFn: (v: ExecutionAssignment) => api.updateAssignment(school, v.id, { device_id: v.device_id, release_id: v.release_id, enabled: false, expected_revision: v.execution_revision }, createIdempotencyKey()), onSuccess: refresh })
  const error = flows.error || validations.error || releases.error || assignments.error || devices.error || register.error || validate.error || release.error || revoke.error || assign.error || disable.error
  const errors = error && <Alert type="error" title={error.message} />
  return <Space orientation="vertical" style={{ width: '100%' }}>
    <Alert type="info" showIcon title="按四步准备手机执行" description="登记流程 → 手机接受本校授权并验证（不发送消息）→ 管理员发布 → 分配给本校设备。这里的验证是设备界面观察，不证明真实账号、真实群身份或消息送达。" />
    {errors}
    <Card title="1. 登记执行流程" extra={admin && <Button onClick={() => setRegisterOpen(true)}>登记新流程</Button>}>
      <Table rowKey="id" loading={flows.isFetching} dataSource={flows.data?.items ?? []} pagination={false} columns={[{ title: '流程名称', dataIndex: 'name' }, { title: '登记时间', dataIndex: 'created_at' }]} locale={{ emptyText: '请由流程维护人员提供新版流程文件，登记后还需手机验证。' }} />
      <PaginationControls page={flowPage} more={flows.data?.page?.has_more} change={setFlowPage} subject="执行流程" />
    </Card>
    <Card title="2. 手机现场验证" extra={admin && <Button onClick={() => setValidateOpen(true)}>创建手机验证</Button>}>
      <Alert type="info" title="先在手机上接受本校用途授权，再创建验证" description="验证只观察界面与首群定位，不点击发送。设备报告通过后 24 小时内可发布；过期或授权变化需重新验证。新版手机验证功能尚在接入，旧手机端不会执行这些验证。" />
      <Table<FlowValidation> rowKey="id" loading={validations.isFetching} dataSource={validations.data?.items ?? []} pagination={false} columns={[
        { title: '流程', dataIndex: 'flow_name' }, { title: '设备', dataIndex: 'device_name' }, { title: '状态', render: (_, v) => validationLabel(v) },
        { title: '设备说明', render: (_, v) => v.result?.evidence || '尚无设备回报' },
        ...(admin ? [{ title: '操作', render: (_: unknown, v: FlowValidation) => <Button disabled={v.state !== 'passed'} loading={release.isPending} onClick={() => Modal.confirm({ title: '发布此设备已验证的流程？', content: '发布后本校设备可选择该流程；其他设备仍需各自完成验证。', onOk: () => release.mutateAsync(v) })}>发布流程</Button> }] : []),
      ]} />
      <PaginationControls page={validationPage} more={validations.data?.page?.has_more} change={setValidationPage} subject="手机验证" />
    </Card>
    <Card title="3. 已发布流程">
      <Table<FlowRelease> rowKey="id" dataSource={releases.data?.items ?? []} pagination={false} columns={[
        { title: '发布名称', dataIndex: 'name' }, { title: '状态', render: (_, v) => <Tag>{v.enabled ? '可分配' : '已撤销'}</Tag> },
        ...(admin ? [{ title: '操作', render: (_: unknown, v: FlowRelease) => <Button danger disabled={!v.enabled} onClick={() => Modal.confirm({ title: '撤销此发布？', content: '关联配置将不可用。已经发出的调用许可仍须按原调用核对。', onOk: () => revoke.mutateAsync(v) })}>撤销发布</Button> }] : []),
      ]} />
      <PaginationControls page={releasePage} more={releases.data?.page?.has_more} change={setReleasePage} subject="流程发布" />
    </Card>
    <Card title="4. 本校设备执行配置" extra={admin && <Button onClick={() => { setEditing(undefined); assignmentForm.resetFields(); setAssignOpen(true) }}>配置设备流程</Button>}>
      <Table<ExecutionAssignment> rowKey="id" dataSource={assignments.data?.items ?? []} pagination={false} columns={[
        { title: '设备', dataIndex: 'device_name' }, { title: '执行流程', dataIndex: 'release_name' },
        { title: '准备状态', render: (_, v) => !v.enabled ? '配置已停用' : v.consent_required ? '请在手机接受当前学校授权' : v.device_validation_required ? '请为此设备验证当前流程' : v.available ? '配置准备完成，任务链路待接入' : '设备授权或流程发布已失效' },
        ...(admin ? [{ title: '操作', render: (_: unknown, v: ExecutionAssignment) => <Space><Button onClick={() => { setEditing(v); assignmentForm.setFieldsValue(v); setAssignOpen(true) }}>更换流程/启用</Button><Button disabled={!v.enabled} onClick={() => Modal.confirm({ title: '停用设备执行配置？', content: '后续新执行将被阻止。', onOk: () => disable.mutateAsync(v) })}>停用</Button></Space> }] : []),
      ]} />
      <PaginationControls page={assignmentPage} more={assignments.data?.page?.has_more} change={setAssignmentPage} subject="执行配置" />
    </Card>
    <Modal title="登记新版执行流程" open={registerOpen} confirmLoading={register.isPending} onCancel={() => setRegisterOpen(false)} onOk={() => void registerForm.validateFields().then(v => register.mutate(v))}>
      {errors}<Form form={registerForm} layout="vertical"><Form.Item name="name" label="流程名称" rules={[{ required: true }]}><Input /></Form.Item><Form.Item name="document" label="流程维护人员提供的流程文件内容（JSON）" rules={[{ required: true }]}><Input.TextArea rows={10} /></Form.Item></Form>
    </Modal>
    <Modal title="创建手机验证" open={validateOpen} confirmLoading={validate.isPending} onCancel={() => setValidateOpen(false)} onOk={() => void validationForm.validateFields().then(v => validate.mutate(v))}>
      {errors}<Form form={validationForm} layout="vertical"><Form.Item name="flow_version_id" label="执行流程" rules={[{ required: true }]}><Select options={flows.data?.items.map(f => ({ value: f.id, label: f.name }))} /></Form.Item><Form.Item name="device_id" label="本校授权设备" rules={[{ required: true }]}><Select options={devices.data?.items.filter(d => d.enabled).map(d => ({ value: d.id, label: d.school_alias || d.name }))} /></Form.Item><Form.Item name="search_text" label="现场测试群搜索文字（只验证，不发送）" rules={[{ required: true }]}><Input /></Form.Item></Form>
      <PaginationControls page={flowPage} more={flows.data?.page?.has_more} change={setFlowPage} subject="验证流程候选" /><PaginationControls page={devicePage} more={devices.data?.page?.has_more} change={setDevicePage} subject="验证设备候选" />
    </Modal>
    <Modal title="配置本校设备流程" open={assignOpen} confirmLoading={assign.isPending} onCancel={() => setAssignOpen(false)} onOk={() => void assignmentForm.validateFields().then(v => assign.mutate(v))}>
      {errors}<Form form={assignmentForm} layout="vertical"><Form.Item name="device_id" label="本校授权设备" rules={[{ required: true }]}><Select disabled={!!editing} options={devices.data?.items.filter(d => d.enabled).map(d => ({ value: d.id, label: d.school_alias || d.name }))} /></Form.Item><Form.Item name="release_id" label="已发布流程" rules={[{ required: true }]}><Select options={releases.data?.items.filter(r => r.enabled).map(r => ({ value: r.id, label: r.name }))} /></Form.Item></Form>
      <PaginationControls page={releasePage} more={releases.data?.page?.has_more} change={setReleasePage} subject="配置流程候选" /><PaginationControls page={devicePage} more={devices.data?.page?.has_more} change={setDevicePage} subject="配置设备候选" />
    </Modal>
  </Space>
}

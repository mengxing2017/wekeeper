import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Input, Modal, Select, Space, Steps, Table, Tag } from 'antd'
import { api, type CapabilityOpening } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

type Request = { kind: 'open' | 'cancel' | 'support' | 'support-update'; row: CapabilityOpening; key: string; note?: string; response?: string; supportState?: 'investigating' | 'resolved' | 'declined' }
const supportStates = { submitted: '已提交', investigating: '处理中', resolved: '已解决', declined: '暂不支持' }
function stage(row: CapabilityOpening) {
  if (!row.release_count) return '产品尚未提供'
  if (!row.environment_fresh) return '等待手机核对环境'
  if (row.matching_releases.length !== 1) return '实际环境暂未适配'
  if (!row.opening) return row.configuration_revision ? '需重新开通' : '等待发送手机确认'
  if (!row.release_available || row.opening.environment_snapshot_id !== row.environment?.id) return '需重新开通'
  return row.consent_current ? '本机已同意，等待现场检查' : '等待本机确认'
}
export function DeviceCapabilities() {
  const client = useQueryClient()
  const [page, setPage] = useState(1)
  const [selectedId, setSelectedId] = useState<string>()
  const [supportState, setSupportState] = useState<'investigating' | 'resolved' | 'declined'>('investigating')
  const [response, setResponse] = useState('')
  const [pending, setPending] = useState<Request>()
  const [note, setNote] = useState('请适配此共享手机的企业微信群文字操作能力。')
  const rows = useQuery({ queryKey: ['mobile-execution', 'platform', 'openings', page], queryFn: () => api.capabilityOpenings(page), refetchInterval: 15000 })
  const selected = rows.data?.items.find(row => row.id === selectedId)
  const save = useMutation<unknown, Error, Request>({ mutationFn: (request: Request) => {
    const { row } = request
    if (request.kind === 'support-update') return api.updateAdapterSupport(row.support_request!.id, { expected_revision: row.support_request!.revision, state: request.supportState!, response: request.response! }, request.key)
    if (request.kind === 'support') return api.requestAdapterSupport({ device_id: row.id, note: request.note! }, request.key)
    if (request.kind === 'cancel') return api.cancelOpening(row.opening!.id, row.opening!.revision, request.key)
    return api.openCapability({ device_id: row.id, profile_release_id: row.matching_releases[0].id, environment_snapshot_id: row.environment!.id, expected_revision: row.configuration_revision }, request.key)
  }, onSuccess: () => { setPending(undefined); void client.invalidateQueries({ queryKey: ['mobile-execution'] }) } })
  function submit(kind: Request['kind'], row: CapabilityOpening) {
    const key = kind === 'open' ? `open:${row.id}:${row.environment!.id}:${row.configuration_revision}` : kind === 'cancel' ? `cancel:${row.opening!.id}:${row.opening!.revision}` : createIdempotencyKey()
    const request: Request = { kind, row, key, note, response, supportState }
    setPending(request); save.mutate(request)
  }
  return <Card title="业务能力开通" extra={<Button loading={rows.isFetching} onClick={() => { void rows.refetch() }}>刷新开通进度</Button>}>
    <Alert type="info" showIcon title="选择手机，按进度完成业务能力开通" description="系统负责提供适配并核对手机实际应用。开通申请、本机同意、现场检查和任务执行分别核对。手机在线不代表可以发送。" />
    {rows.error && <Alert type="error" title="读取开通进度失败" description={rows.error.message} />}
    <Table<CapabilityOpening> rowKey="id" loading={rows.isFetching} dataSource={rows.data?.items ?? []} pagination={false} columns={[
      { title: '共享手机', dataIndex: 'device_name' }, { title: '业务能力', render: () => '企业微信 · 群文字操作' },
      { title: '连接', render: (_, row) => <Tag>{!row.device_enabled ? '设备已停用' : row.online ? '在线' : '离线'}</Tag> },
      { title: '开通进度', render: (_, row) => stage(row) },
      { title: '当前阻塞与负责方', render: (_, row) => row.blockers[0] ? `${row.blockers[0].message}（${row.blockers[0].owner}）` : '等待核对' },
      { title: '最近环境核对', render: (_, row) => row.observed_at ? new Date(row.observed_at).toLocaleString() : '尚未核对' },
      { title: '操作', render: (_, row) => <Button disabled={!!pending} onClick={() => { save.reset(); setResponse(''); setSelectedId(row.id) }}>查看进度与下一步</Button> },
    ]} />
    <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="业务能力开通" />
    <Modal title={selected ? `${selected.device_name} · 业务能力开通` : '业务能力开通'} open={!!selectedId} footer={null} closable={!pending} mask={{ closable: !pending }} onCancel={() => setSelectedId(undefined)}>
      {selected && <>
        <Steps size="small" current={!selected.environment_fresh ? 0 : !selected.opening ? 1 : !selected.consent_current ? 2 : 3} items={['核对手机环境', '发送开通申请', '本机原生确认', '现场检查与开通'].map(title => ({ title }))} />
        <p>业务能力：企业微信 · 群文字操作。手机可服务已绑定的不同学校目标群；每次学校任务仍需独立确认。</p>
        <p>当前进度：{stage(selected)}。手机当前本地状态须由现场人员核对，历史同意回执不代表当前可执行。</p>
        <p>实际应用：{selected.environment?.snapshot.version_name ? `企业微信 ${selected.environment.snapshot.version_name}` : '尚未确认'}；环境{selected.environment_fresh ? '已核对' : '待刷新'}。</p>
        {selected.matching_releases.length === 1 && <p>适用产品适配：{selected.matching_releases[0].name}；负责方：{selected.matching_releases[0].owner}。</p>}
        {selected.blockers.map(blocker => <Alert key={blocker.code} type="warning" title={blocker.message} description={`负责方：${blocker.owner}。下一步：${blocker.next_action}。`} />)}
        {selected.can_open && <Button type="primary" disabled={!!pending} onClick={() => Modal.confirm({ title: '发送到这台手机确认？', content: '系统将固定当前适配和环境。本次只申请本机确认，不启用任务发送，不代替学校授权。', onOk: () => submit('open', selected) })}>发送到手机确认</Button>}
        {selected.opening && <Button danger disabled={!!pending} onClick={() => Modal.confirm({ title: '取消本次开通申请？', content: '手机将不能继续接受该申请。已提交的决定与证据保留；取消不会重置未决执行。', onOk: () => submit('cancel', selected) })}>取消开通申请</Button>}
        {selected.support_request && <Alert type="info" title={`适配支持请求：${supportStates[selected.support_request.state]}`} description={`负责方：${selected.support_request.owner}。${selected.support_request.response || '请求已记录，等待产品研发处理。'} 请求编号：${selected.support_request.id}`} />}
        {selected.support_request && ['submitted', 'investigating'].includes(selected.support_request.state) && <details><summary>登记支持处理进度</summary>
          <p>供负责方登记处理记录；这不会发布适配或开放任务执行。</p>
          <Select aria-label="支持处理状态" value={supportState} onChange={setSupportState} disabled={!!pending} options={[{value:'investigating',label:'处理中'},{value:'resolved',label:'已解决'},{value:'declined',label:'暂不支持'}]} />
          <Input.TextArea aria-label="支持处理说明" value={response} maxLength={2000} disabled={!!pending} onChange={event => setResponse(event.target.value)} />
          <Button disabled={!!pending || !response.trim()} onClick={() => submit('support-update', selected)}>保存支持进度</Button>
        </details>}
        {(!selected.support_request || ['resolved', 'declined'].includes(selected.support_request.state)) && <>
          <p>此业务能力不支持或尚未提供时，可向产品研发提交支持请求。不需要提供聊天内容。</p>
          <Input.TextArea aria-label="适配支持说明" value={note} maxLength={2000} disabled={!!pending} onChange={event => setNote(event.target.value)} />
          <Button disabled={!!pending || !note.trim()} onClick={() => submit('support', selected)}>提交适配支持请求</Button>
        </>}
      </>}
      {save.error && <Alert type="error" title="请求结果尚未确认" description={`${save.error.message}。请重试原请求；系统按同一申请恢复进度。`} />}
      {pending && <Space><Button loading={save.isPending} onClick={() => save.mutate(pending)}>重试同一请求</Button><Button disabled={save.isPending} onClick={() => { setPending(undefined); void rows.refetch() }}>重新查询平台进度</Button></Space>}
    </Modal>
  </Card>
}

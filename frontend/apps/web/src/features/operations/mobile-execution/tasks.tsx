import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Checkbox, Input, Modal, Space, Table, Tag } from 'antd'
import { api, type ExecutionTaskDetail, type MobileDevice, type MobileTarget, type TaskPreview, type TaskPreviewInput } from './api'
import { formatDate } from '../../../shared/format-date'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { PaginationControls } from '../../../shared/pagination-controls'
import { canCancelTask, canConfirmPreview, messageProblem, taskCounts, taskError, taskItemLabels, taskLabels } from './task-model'

type Props = { school: string; admin: boolean; onNavigate?: (view: string) => void }
export function MobileExecutionTasks(props: Props) {
  // School or privilege changes discard all candidates, dialogs and pending presentation state.
  return <TaskWorkspace key={`${props.school}:${props.admin}`} {...props} />
}
function TaskWorkspace({ school, admin, onNavigate }: Props) {
  const cache = useQueryClient()
  const [devicePage, setDevicePage] = useState(1)
  const [targetPage, setTargetPage] = useState(1)
  const [taskPage, setTaskPage] = useState(1)
  const [search, setSearch] = useState('')
  const [device, setDevice] = useState<MobileDevice>()
  const [selected, setSelected] = useState<MobileTarget[]>([])
  const [message, setMessage] = useState('')
  const [preview, setPreview] = useState<TaskPreview>()
  const [detail, setDetail] = useState<ExecutionTaskDetail>()
  const [cancelTarget, setCancelTarget] = useState<ExecutionTaskDetail>()
  const [notice, setNotice] = useState('')
  const [noticeWarning, setNoticeWarning] = useState(false)
  const previewRequest = useRef<{ payload: string; key: string } | undefined>(undefined)
  const alive = useRef(true)
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])
  const devices = useQuery({ queryKey: ['mobile-execution', school, 'devices', devicePage], queryFn: () => api.devices(school, devicePage) })
  const configuration = useQuery({ queryKey: ['mobile-execution', school, 'task-configuration', device?.id], queryFn: () => api.assignments(school, 1, device!.id), enabled: !!device })
  const targets = useQuery({ queryKey: ['mobile-execution', school, 'task-targets', device?.id, search, targetPage], queryFn: () => api.targets(school, targetPage, search, device!.id), enabled: !!device })
  const tasks = useQuery({ queryKey: ['mobile-execution', school, 'tasks', taskPage], queryFn: () => api.tasks(school, taskPage) })
  const assignment = configuration.data?.items[0]
  const ready = !configuration.isFetching && !configuration.isError && assignment?.available === true
  const limit = 20
  const refresh = () => cache.invalidateQueries({ queryKey: ['mobile-execution', school] })
  const buildPreview = useMutation({ mutationFn: (input: TaskPreviewInput) => {
    const payload = JSON.stringify(input)
    if (previewRequest.current?.payload !== payload) previewRequest.current = { payload, key: createIdempotencyKey() }
    return api.taskPreview(school, input, previewRequest.current.key)
  }, onSuccess: result => {
    if (!alive.current) return
    previewRequest.current = undefined
    setPreview(result); setNotice(''); setNoticeWarning(false)
  } })
  const confirm = useMutation({ mutationFn: (p: TaskPreview) => api.confirmTask(school, { preview_id: p.id, preview_digest: p.digest, confirmed: true }, `mobile-task-confirm:${p.id}`), onSuccess: () => {
    if (!alive.current) return
    setPreview(undefined); setSelected([]); setMessage(''); setTaskPage(1)
    setNoticeWarning(false); setNotice('任务已登记并预留手机。当前暂未开放手机发送；可在任务记录查看或取消尚未领取的任务。')
    void refresh()
  } })
  const cancel = useMutation({ mutationFn: (task: ExecutionTaskDetail) => api.cancelTask(school, task.id, task.revision, '使用者取消尚未领取的手机任务', `mobile-task-cancel:${task.id}:${task.revision}`), onSuccess: () => {
    if (!alive.current) return
    setCancelTarget(undefined); setDetail(undefined); setNoticeWarning(false); setNotice('未领取任务已取消，手机预留已释放。'); void refresh()
  }, onError: () => { void refresh() } })
  const busy = buildPreview.isPending || confirm.isPending
  const frozen = busy || !!preview
  const queryError = devices.error || configuration.error || targets.error || tasks.error
  const blocked = !assignment ? '尚未配置执行流程，请联系学校管理员在“手机执行配置”完成配置。'
    : !assignment.enabled ? '执行配置已停用，请联系学校管理员检查。'
      : assignment.consent_required ? '请先在手机上接受当前学校用途授权。'
        : assignment.device_validation_required ? '请先在手机完成当前流程的现场验证，再由学校管理员检查发布和配置。'
          : '设备授权或流程发布已失效，请联系学校管理员检查。'
  const error = buildPreview.error || confirm.error || cancel.error || queryError
  const closePreview = () => {
    if (confirm.error) { setNoticeWarning(true); setNotice('确认结果尚未核实。关闭预览不会取消可能已经登记的任务，请先刷新任务记录，不要重复创建。') }
    setPreview(undefined); confirm.reset(); void tasks.refetch()
  }
  const denied = [buildPreview.error, confirm.error, cancel.error, devices.error, configuration.error, targets.error, tasks.error].find(e => e && typeof e === 'object' && 'code' in e && ['forbidden', 'unauthorized'].includes(String(e.code)))
  const requestPreview = () => {
    if (!device || !ready || !selected.length || messageProblem(message)) return
    confirm.reset(); buildPreview.reset()
    buildPreview.mutate({ device_id: device.id, target_ids: selected.map(t => t.id), message })
  }
  const move = (index: number, delta: number) => setSelected(items => {
    const next = [...items]; [next[index], next[index + delta]] = [next[index + delta], next[index]]; return next
  })
  if (denied) return <Alert type="error" title={taskError(denied)} description="请离开此页并重新选择学校或重新登录。旧学校的候选与确认不会继续显示。" />
  return <Space orientation="vertical" style={{ width: '100%' }}>
    <Alert showIcon type="warning" title="当前仅支持手机任务登记，暂未开放手机发送" description="依次选择手机、选择已绑定目标、填写正文、预览全部内容，再由本校管理员确认。确认会预留手机，不会启动手机发送。官方客户群群发仍在原有入口独立使用。" />
    {!admin && <Alert type="info" title="运营人员可以预览和取消未领取任务；人工确认请由本校管理员操作。" />}
    {notice && <Alert type={noticeWarning ? 'warning' : 'success'} title={notice} />}
    {error && <Alert type="error" title={taskError(error)} />}
    <Button disabled={busy} onClick={() => { closePreview(); buildPreview.reset(); cancel.reset(); void refresh() }}>刷新配置、目标和任务</Button>
    <Card title="1. 选择本校手机">
      <Table<MobileDevice> rowKey="id" pagination={false} loading={devices.isFetching} dataSource={devices.data?.items ?? []} locale={{ emptyText: '本校暂无授权手机，请联系平台管理员授予本校设备使用权。' }} columns={[
        { title: '手机', render: (_, d) => d.school_alias || d.name },
        { title: '连接状态（不代表发送资格）', render: (_, d) => d.control_state === 'online' ? '控制连接在线' : '当前不在线或连接待核实' },
        { title: '选择', render: (_, d) => <Button disabled={frozen || !d.enabled || device?.id === d.id} onClick={() => { setDevice(d); setSelected([]); setTargetPage(1); setSearch(''); buildPreview.reset() }}>{device?.id === d.id ? '已选择' : `使用${d.school_alias || d.name}`}</Button> },
      ]} />
      <PaginationControls page={devicePage} more={devices.data?.page?.has_more} busy={frozen || devices.isFetching} change={setDevicePage} subject="任务手机" />
      {device && <p>已选手机：{device.school_alias || device.name}</p>}
      {device && !configuration.isPending && !ready && <Alert type="warning" title={blocked} action={onNavigate && <Button onClick={() => onNavigate('mobile-execution-flows')}>查看手机执行配置</Button>} />}
    </Card>
    <Card title="2. 选择目标并核对顺序">
      {!device ? <p>请先选择手机，仅显示这台手机在本校有效绑定的目标。</p> : <>
        <Input.Search key={device.id} aria-label="搜索可选目标" placeholder="按下发群名或别名搜索" disabled={frozen} onSearch={value => { setSearch(value); setTargetPage(1) }} allowClear />
        <p>已选择 {selected.length} / {limit} 个目标，跨页保留选择。选择本页不会选择其他页；实际数量还受所选流程预算约束。</p>
        <Space><Button disabled={frozen || targets.isFetching || targets.isError} onClick={() => setSelected(items => [...items, ...(targets.data?.items ?? []).filter(t => t.enabled && !items.some(i => i.id === t.id))].slice(0, limit))}>选择本页（最多 {limit} 个）</Button><Button disabled={frozen || !selected.length} onClick={() => setSelected([])}>清空选择</Button></Space>
        <Table<MobileTarget> rowKey="id" pagination={false} loading={targets.isFetching} dataSource={targets.data?.items ?? []} rowSelection={{ selectedRowKeys: selected.map(t => t.id), preserveSelectedRowKeys: true, hideSelectAll: true,
          getCheckboxProps: row => ({ disabled: frozen || !row.enabled || (selected.length >= limit && !selected.some(t => t.id === row.id)), 'aria-label': `选择目标${row.alias || row.search_text}` }),
          onSelect: (row, checked) => setSelected(items => checked ? items.some(t => t.id === row.id) ? items : [...items, row] : items.filter(t => t.id !== row.id)),
        }} columns={[{ title: '下发群名（原文）', render: (_, t) => <span style={{ whiteSpace: 'pre-wrap' }}>{t.search_text}</span> }, { title: '平台别名', dataIndex: 'alias' }]} locale={{ emptyText: search ? '没有匹配目标，请调整搜索；已选目标仍然保留。' : '该手机尚无可用目标，请联系学校管理员在“手机目标群”新增并绑定。' }} />
        <PaginationControls page={targetPage} more={targets.data?.page?.has_more} busy={frozen || targets.isFetching} change={setTargetPage} subject="任务目标" />
        {onNavigate && <Button disabled={frozen} onClick={() => onNavigate('mobile-targets')}>查看手机目标群与绑定</Button>}
        <ol aria-label="已选目标执行顺序">{selected.map((t, index) => <li key={t.id}><span style={{ whiteSpace: 'pre-wrap' }}>{t.search_text}</span> <Space><Button size="small" aria-label={`上移目标${index + 1}`} disabled={frozen || index === 0} onClick={() => move(index, -1)}>上移</Button><Button size="small" aria-label={`下移目标${index + 1}`} disabled={frozen || index === selected.length - 1} onClick={() => move(index, 1)}>下移</Button><Button size="small" aria-label={`移除目标${index + 1}`} disabled={frozen} onClick={() => setSelected(items => items.filter(i => i.id !== t.id))}>移除</Button></Space></li>)}</ol>
      </>}
    </Card>
    <Card title="3. 填写消息并生成预览">
      <Input.TextArea aria-label="手机任务消息正文" rows={5} value={message} disabled={frozen} onChange={e => { setMessage(e.target.value); buildPreview.reset() }} />
      <p>{new TextEncoder().encode(message).length} / 4000 UTF-8 字节，正文按原样保存，不裁剪空格或换行。</p>
      {message && messageProblem(message) && <Alert type="warning" title={messageProblem(message)} />}
      <Button type="primary" loading={buildPreview.isPending} disabled={frozen || !ready || !selected.length || !!messageProblem(message) || targets.isError} onClick={requestPreview}>预览全部目标和正文</Button>
    </Card>
    <Card title="手机任务记录" extra={<Button onClick={() => { void tasks.refetch() }}>刷新任务记录</Button>}>
      <Table<ExecutionTaskDetail> rowKey="id" pagination={false} loading={tasks.isFetching} dataSource={tasks.data?.items ?? []} columns={[
        { title: '登记时间', dataIndex: 'created_at', render: value => formatDate(value) }, { title: '手机（当前名称）', dataIndex: 'device_name' }, { title: '目标', render: (_, t) => `${t.plan.items.slice(0, 2).map(i => i.search_text).join('、')}${t.plan.items.length > 2 ? '…' : ''}（共 ${t.plan.items.length} 个）` }, { title: '任务状态', render: (_, t) => <Tag>{taskLabels[t.state]}</Tag> },
        { title: '逐项统计', render: (_, t) => taskCounts(t) }, { title: '操作', render: (_, t) => <Space><Button onClick={() => setDetail(t)}>查看原计划和逐项结果</Button>{canCancelTask(t) && <Button danger onClick={() => { cancel.reset(); setCancelTarget(t) }}>取消未领取任务</Button>}</Space> },
      ]} locale={{ emptyText: '本校暂无新版手机任务。先选择手机和目标，再生成预览。' }} />
      <PaginationControls page={taskPage} more={tasks.data?.page?.has_more} busy={tasks.isFetching} change={setTaskPage} subject="手机任务" />
    </Card>
    {preview && <PreviewDialog key={preview.id} preview={preview} admin={admin} deviceName={device?.school_alias || device?.name || ''} pending={confirm.isPending} error={confirm.error} onClose={closePreview} onConfirm={() => confirm.mutate(preview)} />}
    <Modal title="原计划与逐项结果" open={!!detail} width={850} footer={null} onCancel={() => setDetail(undefined)}>
      {detail && <><p>手机（当前名称）：{detail.device_name}；{taskCounts(detail)}</p><Plan snapshot={detail.plan} /><Table rowKey="id" pagination={false} dataSource={detail.items} columns={[{ title: '顺序', dataIndex: 'position' }, { title: '原下发群名', render: (_, i) => <span style={{ whiteSpace: 'pre-wrap' }}>{i.snapshot.search_text}</span> }, { title: '状态', render: (_, i) => taskItemLabels[i.state] }]} /><Alert type="info" title="报告已发只代表设备观察，不代表送达；不确定结果须核对，不能直接重新发送。" /></>}
    </Modal>
    <Modal title="取消尚未领取的任务？" open={!!cancelTarget} confirmLoading={cancel.isPending} okText="确认取消任务" cancelText="保留任务" onCancel={() => { if (!cancel.isPending) setCancelTarget(undefined) }} onOk={() => cancelTarget && cancel.mutate(cancelTarget)}>
      <p>仅当全部目标仍未领取时取消并释放手机预留；已发调用或不确定结果不能通过此操作清除。</p>{cancel.error && <Alert type="error" title={taskError(cancel.error)} />}
    </Modal>
  </Space>
}
function Plan({ snapshot }: { snapshot: ExecutionTaskDetail['plan'] }) {
  const c = snapshot.configuration
  return <>
    <p>应用：{c.flow.application_label}；群间最短间隔 {c.flow.pacing.between_targets_min_ms / 1000} 秒。</p>
    <h4>消息正文（原文）</h4><pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{snapshot.message}</pre>
    <Table rowKey="target_id" pagination={false} dataSource={snapshot.items} columns={[{ title: '执行顺序', dataIndex: 'position' }, { title: '下发群名（原文）', render: (_, i) => <span style={{ whiteSpace: 'pre-wrap' }}>{i.search_text}</span> }]} />
    <details><summary>查看确认版本</summary><p>学校授权 {c.grant_revision}；手机同意 {c.consent_revision}；执行配置 {c.assignment_revision}；流程发布 {c.release_revision}</p><p>流程摘要：{c.flow_digest}</p><ul>{snapshot.items.map(i => <li key={i.target_id}>{i.position}. 定位版本 {i.locator_revision}，绑定版本 {i.binding_revision}</li>)}</ul></details>
  </>
}

function PreviewDialog({ preview, admin, deviceName, pending, error, onClose, onConfirm }: { preview: TaskPreview; admin: boolean; deviceName: string; pending: boolean; error: Error | null; onClose: () => void; onConfirm: () => void }) {
  const [reviewed, setReviewed] = useState(false)
  const [attempted, setAttempted] = useState(false)
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [])
  const expired = !(Date.parse(preview.expires_at) > now)
  return <Modal title="4. 核对全部目标并确认任务" open width={850} footer={null} closable={!pending} mask={{ closable: false }} onCancel={() => { if (!pending) onClose() }}>
    <Alert type="warning" title="手机按群名搜索，只选择群分类中视觉顺序的第一个结果" description="请先维护并现场核实手机内群名备注。平台不识别真实账号、真实群身份，也不保证消息送达。当前确认仅登记任务并预留手机，不会启动发送。" />
    <p>手机：{deviceName}；共 {preview.snapshot.items.length} 个目标；群间最短间隔 {preview.snapshot.configuration.flow.pacing.between_targets_min_ms / 1000} 秒。</p>
    <p>预览有效至：{formatDate(preview.expires_at)}</p>
    {expired && <Alert type="warning" title={attempted ? '预览已过期；仅可重试已发起的同一次确认，或关闭后核实任务记录' : '预览已过期，请关闭并重新预览'} />}
    {error && <Alert type="error" title={taskError(error)} description="网络异常时请重试同一次确认；重试不会新增第二个任务。关闭预览不会取消可能已经登记的任务，请刷新任务记录核实。" />}
    <Plan snapshot={preview.snapshot} />
    <Checkbox checked={reviewed} disabled={pending} onChange={e => setReviewed(e.target.checked)}>已核对全部目标、顺序、原始群名及消息内容</Checkbox>
    <Space><Button disabled={pending} onClick={onClose}>{attempted ? '关闭并检查任务记录' : '返回修改'}</Button>{admin ? <Button type="primary" loading={pending} disabled={!reviewed || !canConfirmPreview(preview.expires_at, now, attempted) || pending} onClick={() => { if (reviewed && canConfirmPreview(preview.expires_at, Date.now(), attempted)) { setAttempted(true); onConfirm() } }}>{error ? '重试同一次确认' : '确认任务'}</Button> : <p>请由本校管理员重新预览并人工确认。</p>}</Space>
  </Modal>
}

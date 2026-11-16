import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, Input, Modal, Select, Space, Table, Tag } from 'antd'
import {
  dispatchRecords,
  type DispatchQuery,
  type DispatchPage,
  type DispatchIntake,
  type DispatchOperation,
  type DispatchCheck,
  type DispatchEvent,
  type DeliveryExchange,
} from './api'
import { formatDate } from '../../../shared/format-date'
import { TableIdentifier } from '../../../shared/table-text'
const stageLabels: Record<string, string> = {
  registered: '已进入领域，尚未开始创建',
  processing: '任务创建中',
  retrying: '创建异常，等待重试',
  rejected: '任务创建失败',
  round_wait: '候选耗尽，等待轮次重评',
  awaiting_pull: '等待手机拉取',
  awaiting_receipt: '包已生成，尚无落盘接收确认',
  executing: '手机已接收，等待最终结果',
  sent: '已发送',
  failed: '已结束：失败',
  unknown: '结果待核对',
  completed: '已发送',
  stopped: '已停止',
  confirmation_expired: '授权已到期',
  dispatch_failed: '三轮下发已耗尽',
  needs_review: '结果待核对',
}
const responseLabels: Record<string, string> = {
  not_started: '尚未开始响应观测',
  response_started: '响应流处理中',
  server_write_completed: '服务端写出完成，未证明手机接收',
  server_write_interrupted: '服务端响应流中断',
  response_observation_unknown: '响应观测不完整',
}
const resultLabels: Record<string, string> = {
  started: '处理未完成',
  created: '任务创建成功',
  retrying: '等待重试',
  rejected: '已拒绝',
  failed: '失败',
  passed: '检查通过',
  skipped: '已跳过',
  assigned: '已分配',
  executing: '等待结果',
  waiting_resources: '等待资源',
  granted: '包已生成',
  idle: '未交付任务',
  waiting: '等待',
  interrupted: '处理中断',
  superseded: '分配已被替代',
}
function ErrorState({ error }: { error: unknown }) {
  return error ? (
    <Alert
      type="error"
      title="记录读取失败，不能据此判断没有下发记录"
      description={error instanceof Error ? error.message : String(error)}
    />
  ) : null
}
function Evidence({ value }: { value: Record<string, unknown> }) {
  return (
    <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12 }}>{JSON.stringify(value, null, 2)}</pre>
  )
}
function CursorButtons({
  more,
  next,
  previous,
}: {
  more: boolean
  next: () => void
  previous?: () => void
}) {
  return (
    <Space>
      <Button disabled={!previous} onClick={previous}>
        上一页
      </Button>
      <Button disabled={!more} onClick={next}>
        下一页
      </Button>
    </Space>
  )
}
function useCursor() {
  const [cursors, setCursors] = useState<Array<string | undefined>>([undefined])
  return {
    cursor: cursors.at(-1),
    next: (v: string | null) => {
      if (v) setCursors((c) => [...c, v])
    },
    previous: cursors.length > 1 ? () => setCursors((c) => c.slice(0, -1)) : undefined,
    reset: () => setCursors([undefined]),
  }
}
function Paged<T extends { id: string | number }>({
  identity,
  load,
  columns,
}: {
  identity: string
  load: (q: DispatchQuery) => Promise<DispatchPage<T>>
  columns: React.ComponentProps<typeof Table<T>>['columns']
}) {
  const c = useCursor()
  const [reason, setReason] = useState('')
  const [kind, setKind] = useState('')
  const [result, setResult] = useState('')
  const query = useQuery({
    queryKey: ['mobile-dispatch', identity, c.cursor, reason, kind, result],
    queryFn: () =>
      load({
        cursor: c.cursor,
        reason: reason || undefined,
        kind: kind || undefined,
        result: result || undefined,
      }),
    refetchInterval: 5000,
  })
  return (
    <Space orientation="vertical" style={{ width: '100%' }}>
      <Input.Search
        aria-label={`${identity}原因筛选`}
        placeholder="按原因码筛选"
        onSearch={(v) => {
          setReason(v)
          c.reset()
        }}
      />
      {(identity.startsWith('operations-') || identity.startsWith('events-')) && (
        <Input.Search
          aria-label={`${identity}事件类型筛选`}
          placeholder="处理/事件类型"
          onSearch={(v) => {
            setKind(v)
            c.reset()
          }}
        />
      )}
      <Input.Search
        aria-label={`${identity}处理结果筛选`}
        placeholder="处理结果码"
        onSearch={(v) => {
          setResult(v)
          c.reset()
        }}
      />
      <ErrorState error={query.error} />
      {query.data && <small>观察时间：{formatDate(query.data.observed_at)}</small>}
      <Table<T>
        rowKey="id"
        loading={query.isFetching}
        dataSource={query.data?.items ?? []}
        columns={columns}
        pagination={false}
        scroll={{ x: 900 }}
      />
      <CursorButtons
        more={!!query.data?.has_more}
        next={() => c.next(query.data?.next_cursor ?? null)}
        previous={c.previous}
      />
    </Space>
  )
}
function Checks({ id, exchange = false }: { id: string; exchange?: boolean }) {
  return (
    <Paged<DispatchCheck>
      key={id}
      identity={`checks-${id}`}
      load={(q) =>
        exchange ? dispatchRecords.exchangeChecks(id, q) : dispatchRecords.checks(id, q)
      }
      columns={[
        { title: '检查时间', dataIndex: 'created_at', render: (v) => formatDate(v) },
        {
          title: '候选手机',
          dataIndex: 'device_id',
          render: (v) => <TableIdentifier value={v ?? '未关联'} />,
        },
        { title: '检查项', dataIndex: 'check_kind' },
        { title: '决定', dataIndex: 'decision', render: (v) => resultLabels[v] ?? v },
        {
          title: '具体原因',
          render: (_, r) => (
            <>
              <div>{r.reason_code === 'passed' ? '检查通过' : r.reason_label}</div>
              <small>{r.reason_code}</small>
            </>
          ),
        },
        {
          title: '当时证据（未列出的项未检查）',
          dataIndex: 'evidence',
          render: (v) => <Evidence value={v} />,
        },
      ]}
    />
  )
}
function RecordDetail({ id }: { id: string }) {
  const [operation, setOperation] = useState<string>()
  const [exchange, setExchange] = useState<DeliveryExchange>()
  const query = useQuery({
    queryKey: ['mobile-dispatch', 'detail', id],
    queryFn: () => dispatchRecords.detail(id),
    refetchInterval: 5000,
  })
  const d = query.data
  return (
    <Space orientation="vertical" style={{ width: '100%' }}>
      <ErrorState error={query.error} />
      {query.isPending && <p>正在读取下发记录…</p>}
      {d && (
        <>
          <Alert
            showIcon
            type={d.outcome === 'failed' ? 'error' : d.stage === 'round_wait' ? 'warning' : 'info'}
            title={stageLabels[d.stage] ?? d.stage}
            description={
              <>
                <div>
                  {d.reason_label}（{d.reason_code}）
                </div>
                {d.candidate_reasons?.map((c, i) => (
                  <div key={i}>
                    {c.device_name ?? c.device_id ?? '候选手机'}：{c.reason_label}（{c.reason_code}
                    ）
                  </div>
                ))}
                <div>
                  下一步：{d.next_action}（
                  {d.owner === 'admin' ? '系统管理员' : d.owner === 'device' ? '手机' : '系统'}）
                </div>
              </>
            }
          />
          <p>
            {d.target_name}：{d.content}
          </p>
          <p>
            任务：{d.task_id ?? '尚未创建'}；当前手机：{d.device_name ?? '尚未选定'}；尝试：
            {d.attempt_no ?? '暂无'}
          </p>
          <p>
            进入领域：{formatDate(d.created_at)}；执行包生成：
            {d.generated_at ? formatDate(d.generated_at) : '未观察到'}；手机落盘接收：
            {d.received_at ? formatDate(d.received_at) : '暂无接收确认'}
          </p>
          <p>
            最近真实进展：{formatDate(d.last_progress_at)}；最近检查：
            {d.last_checked_at ? formatDate(d.last_checked_at) : '尚未检查'}；观察时间：
            {formatDate(d.observed_at)}
          </p>
          <p>
            当前轮次：
            {d.max_rounds
              ? `${Math.min((d.completed_rounds ?? 0) + 1, d.max_rounds)}/${d.max_rounds}`
              : '尚未调度'}
            ；轮次截止：{formatDate(d.round_deadline_at)}；下次重评：
            {formatDate(d.next_evaluation_at)}；结果截止：{formatDate(d.result_deadline_at)}
          </p>
          {d.historical_risk && (
            <Alert type="warning" title="原尝试存在未知风险，当前候选继续按授权调度" />
          )}
          <Space>
            {d.task_id && (
              <Button
                href={`/admin?view=mobile-execution-tasks&task=${encodeURIComponent(d.task_id)}`}
              >
                手机任务
              </Button>
            )}
            {d.receipt_id && (
              <Button
                href={`/admin?view=source-automation-tasks&receipt=${encodeURIComponent(d.receipt_id)}`}
              >
                来源消息
              </Button>
            )}
          </Space>
          <Card title="每次创建处理与调度评估">
            <Paged<DispatchOperation>
              identity={`operations-${id}`}
              load={(q) => dispatchRecords.operations(id, q)}
              columns={[
                { title: '开始时间', dataIndex: 'started_at', render: (v) => formatDate(v) },
                {
                  title: '类型',
                  dataIndex: 'kind',
                  render: (v) => (v === 'create' ? '任务创建' : '候选调度评估'),
                },
                { title: '轮次', dataIndex: 'round_no' },
                { title: '结果', dataIndex: 'result', render: (v) => resultLabels[v] ?? v },
                {
                  title: '具体原因',
                  render: (_, r) => (
                    <>
                      {r.reason_label}
                      <small>（{r.reason_code}）</small>
                    </>
                  ),
                },
                { title: '重试时间', dataIndex: 'retry_at', render: (v) => formatDate(v) },
                {
                  title: '候选检查',
                  render: (_, r) => <Button onClick={() => setOperation(r.id)}>逐候选原因</Button>,
                },
              ]}
            />
          </Card>
          <Card title="手机交付交互">
            <Paged<DeliveryExchange>
              identity={`exchanges-${id}`}
              load={(q) => dispatchRecords.exchanges({ ...q, intake_id: id })}
              columns={[
                { title: '手机请求时间', dataIndex: 'created_at', render: (v) => formatDate(v) },
                { title: '处理结果', dataIndex: 'result', render: (v) => resultLabels[v] ?? v },
                { title: '原因', dataIndex: 'reason_label' },
                {
                  title: '服务端响应观测',
                  dataIndex: 'response_state',
                  render: (v) => responseLabels[v] ?? v,
                },
                {
                  title: '是否重放',
                  render: (_, r) => (r.replay_of ? '同一授权重返' : '首次请求'),
                },
                {
                  title: '操作',
                  render: (_, r) => <Button onClick={() => setExchange(r)}>交付检查</Button>,
                },
              ]}
            />
            <small>服务端写出完成不代表手机收到；只有落盘接收确认才证明已接收。</small>
          </Card>
          <Card title="完整事件时间线">
            <Paged<DispatchEvent>
              identity={`events-${id}`}
              load={(q) => dispatchRecords.events(id, q)}
              columns={[
                { title: '发生时间', dataIndex: 'created_at', render: (v) => formatDate(v) },
                { title: '事件', dataIndex: 'kind' },
                { title: '结果', dataIndex: 'result', render: (v) => resultLabels[v] ?? v },
                {
                  title: '原因',
                  render: (_, r) => (
                    <>
                      {r.reason_label}
                      <small>（{r.reason}）</small>
                    </>
                  ),
                },
                {
                  title: '原尝试',
                  dataIndex: 'attempt_id',
                  render: (v) => <TableIdentifier value={v ?? '尚未创建'} />,
                },
                { title: '事实证据', dataIndex: 'evidence', render: (v) => <Evidence value={v} /> },
              ]}
            />
          </Card>
        </>
      )}
      <Modal
        title="本次逐候选检查"
        open={!!operation}
        footer={null}
        width={1100}
        onCancel={() => setOperation(undefined)}
      >
        {operation && <Checks id={operation} />}
      </Modal>
      <Modal
        title="本次交付检查"
        open={!!exchange}
        footer={null}
        width={1100}
        onCancel={() => setExchange(undefined)}
      >
        {exchange && (
          <>
            <p>
              响应：{responseLabels[exchange.response_state] ?? exchange.response_state}；执行包：
              {exchange.execution_id ?? '未生成'}；{exchange.reason_label}
            </p>
            <Checks id={exchange.id} exchange />
          </>
        )}
      </Modal>
    </Space>
  )
}
export function MobileDispatchRecords() {
  const params = new URLSearchParams(window.location.search)
  const [selected, setSelected] = useState<string | undefined>(
    () => params.get('intake') ?? undefined,
  )
  const [q, setQ] = useState<DispatchQuery>(() => ({
    message_id: params.get('message_id') ?? undefined,
    receipt_id: params.get('receipt_id') ?? undefined,
    task_id: params.get('task_id') ?? undefined,
    device_id: params.get('device_id') ?? undefined,
  }))
  const cursor = useCursor()
  const query = useQuery({
    queryKey: ['mobile-dispatch', 'list', q, cursor.cursor],
    queryFn: () => dispatchRecords.list({ ...q, cursor: cursor.cursor }),
    refetchInterval: 5000,
  })
  function filter(key: keyof DispatchQuery, v: string | undefined) {
    setQ((old) => ({ ...old, [key]: v || undefined }))
    cursor.reset()
  }
  return (
    <Card title="自动任务下发记录">
      <Space orientation="vertical" style={{ width: '100%' }}>
        <Alert
          type="info"
          title="进入手机分发领域即留记录，任务尚未创建或手机尚未接收也可查询"
          description="每次候选检查、交付和失败分别记录；读取记录不会补发消息。"
        />
        <Space wrap>
          <Select
            aria-label="记录来源"
            value={q.source_kind ?? 'source_route'}
            onChange={(v) => filter('source_kind', v)}
            options={[
              { value: 'source_route', label: '自动来源' },
              { value: 'manual', label: '手工任务' },
              { value: 'all', label: '全部来源' },
            ]}
          />
          <Input.Search
            aria-label="接入记录筛选"
            placeholder="接入记录 ID"
            onSearch={(v) => filter('intake_id', v)}
          />
          <Input.Search
            aria-label="消息筛选"
            placeholder="消息 ID"
            defaultValue={q.message_id}
            onSearch={(v) => filter('message_id', v)}
          />
          <Input.Search
            aria-label="目标群筛选"
            placeholder="目标群名称或 ID"
            onSearch={(v) => filter('target', v)}
          />
          <Input.Search
            aria-label="消息回执筛选"
            placeholder="来源接收回执 ID"
            defaultValue={q.receipt_id}
            onSearch={(v) => filter('receipt_id', v)}
          />
          <Input.Search
            aria-label="手机筛选"
            placeholder="手机 ID"
            defaultValue={q.device_id}
            onSearch={(v) => filter('device_id', v)}
          />
          <Input.Search
            aria-label="任务筛选"
            placeholder="任务 ID"
            defaultValue={q.task_id}
            onSearch={(v) => filter('task_id', v)}
          />
          <Input.Search
            aria-label="原因筛选"
            placeholder="具体原因码"
            onSearch={(v) => filter('reason', v)}
          />
          <Select
            aria-label="原因范围"
            value={q.reason_scope ?? 'current'}
            onChange={(v) => filter('reason_scope', v)}
            options={[
              { value: 'current', label: '当前原因' },
              { value: 'history', label: '历史曾发生的原因' },
            ]}
          />
          <Select
            aria-label="当前阶段"
            allowClear
            placeholder="当前阶段"
            style={{ minWidth: 180 }}
            value={q.stage}
            onChange={(v) => filter('stage', v)}
            options={Object.entries(stageLabels).map(([value, label]) => ({ value, label }))}
          />
          <Select
            aria-label="最终结果"
            allowClear
            placeholder="最终结果"
            style={{ minWidth: 120 }}
            value={q.outcome}
            onChange={(v) => filter('outcome', v)}
            options={[
              { value: 'pending', label: '处理中' },
              { value: 'sent', label: '成功' },
              { value: 'failed', label: '失败' },
              { value: 'unknown', label: '不确定' },
            ]}
          />
          <label>
            开始时间
            <Input
              type="datetime-local"
              aria-label="开始时间"
              onChange={(e) =>
                filter('from', e.target.value ? new Date(e.target.value).toISOString() : undefined)
              }
            />
          </label>
          <label>
            结束时间
            <Input
              type="datetime-local"
              aria-label="结束时间"
              onChange={(e) =>
                filter('until', e.target.value ? new Date(e.target.value).toISOString() : undefined)
              }
            />
          </label>
        </Space>
        <ErrorState error={query.error} />
        {query.data && <small>本页观察时间：{formatDate(query.data.observed_at)}</small>}
        <Table<DispatchIntake>
          rowKey="id"
          loading={query.isFetching}
          dataSource={query.data?.items ?? []}
          pagination={false}
          scroll={{ x: 1500 }}
          columns={[
            {
              title: '进入手机领域',
              width: 180,
              dataIndex: 'created_at',
              render: (v) => formatDate(v),
            },
            { title: '冻结目标群', dataIndex: 'target_name' },
            { title: '正文摘要', dataIndex: 'content_summary' },
            {
              title: '任务',
              render: (_, r) =>
                r.task_id ? <TableIdentifier value={r.task_id} /> : <Tag>尚未创建</Tag>,
            },
            {
              title: '当前停留位置',
              render: (_, r) => (
                <>
                  <strong>{stageLabels[r.stage] ?? r.stage}</strong>
                  <div>{r.reason_label}</div>
                  <small>原因：{r.reason_code}</small>
                  {r.candidate_reasons?.slice(0, 2).map((c, i) => (
                    <div key={i}>
                      {c.device_name ?? '候选手机'}：{c.reason_label}
                    </div>
                  ))}
                  {(r.candidate_reasons?.length ?? 0) > 2 && <small>其余原因见完整记录</small>}
                </>
              ),
            },
            {
              title: '下一步',
              render: (_, r) => (
                <>
                  {r.next_action}
                  <div>重评：{formatDate(r.next_evaluation_at)}</div>
                </>
              ),
            },
            {
              title: '手机接收',
              render: (_, r) => (r.received_at ? formatDate(r.received_at) : '暂无落盘接收确认'),
            },
            { title: '当前手机', render: (_, r) => r.device_name ?? '尚未选定' },
            {
              title: '操作',
              render: (_, r) => <Button onClick={() => setSelected(r.id)}>完整下发记录</Button>,
            },
          ]}
        />
        <CursorButtons
          more={!!query.data?.has_more}
          next={() => cursor.next(query.data?.next_cursor ?? null)}
          previous={cursor.previous}
        />
        <Card title="设备级交付请求（包含无匹配任务的拉取）">
          <Paged<DeliveryExchange>
            key={q.device_id ?? 'all'}
            identity={`device-exchanges-${q.device_id ?? 'all'}`}
            load={(p) =>
              dispatchRecords.exchanges({
                ...p,
                device_id: q.device_id,
                from: q.from,
                until: q.until,
              })
            }
            columns={[
              { title: '请求时间', dataIndex: 'created_at', render: (v) => formatDate(v) },
              {
                title: '手机',
                dataIndex: 'device_id',
                render: (v) => <TableIdentifier value={v} />,
              },
              { title: '处理结果', dataIndex: 'result', render: (v) => resultLabels[v] ?? v },
              { title: '原因', dataIndex: 'reason_label' },
              {
                title: '响应观测',
                dataIndex: 'response_state',
                render: (v) => responseLabels[v] ?? v,
              },
              {
                title: '目标关联',
                render: (_, r) =>
                  r.intake_id ? (
                    <Button onClick={() => setSelected(r.intake_id!)}>目标下发记录</Button>
                  ) : (
                    '本次没有选中目标'
                  ),
              },
            ]}
          />
        </Card>
      </Space>
      <Modal
        title="手机分发完整记录"
        open={!!selected}
        footer={null}
        width={1300}
        onCancel={() => setSelected(undefined)}
      >
        {selected && <RecordDetail key={selected} id={selected} />}
      </Modal>
    </Card>
  )
}

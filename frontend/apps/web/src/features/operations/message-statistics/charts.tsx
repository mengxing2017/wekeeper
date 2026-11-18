import { useState, type ReactNode } from 'react'
import { Button, Empty, Select, Space, Table } from 'antd'
import type { Bucket, Counts, Breakdown, DetailFilter, StatisticsOverview } from './api'
import { receiptReasonLabels } from './api'
import { ComparisonChart } from '../../../shared/charts/comparison'
import { formatCount } from '../../../shared/charts/numbers'
import {
  channelNames,
  isOutcome,
  outcomes,
  outcomeLabels,
  rankGroups,
  resultSeries,
  successRate,
  type GroupMetric,
  type Outcome,
} from './chart-data'
export const count = formatCount
const formatDate = (value: string) =>
  new Date(value).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })
function bucketLabel(start: string, buckets: Bucket[]) {
  const date = (value: string) =>
    new Date(value).toLocaleDateString('sv-SE', { timeZone: 'Asia/Shanghai' })
  const day = date(start)
  const hourly =
    buckets.length > 1 &&
    new Date(buckets[1].start).getTime() - new Date(buckets[0].start).getTime() < 86400000
  return hourly
    ? `${day.slice(5)} ${new Date(start).toLocaleTimeString('zh-CN', { timeZone: 'Asia/Shanghai', hour: '2-digit', minute: '2-digit' })}`
    : day
}
function DataTable({ children, title = '查看数据表' }: { children: ReactNode; title?: string }) {
  const [expanded, setExpanded] = useState(false)
  return (
    <details onToggle={(event) => setExpanded(event.currentTarget.open)}>
      <summary style={{ cursor: 'pointer', padding: '12px 0' }}>
        {title}（精确数值与明细操作）
      </summary>
      {expanded && children}
    </details>
  )
}

export function Trend({
  buckets,
  onSelect,
}: {
  buckets: Bucket[]
  onSelect: (start: string) => void
}) {
  return (
    <section aria-label="接收消息趋势图">
      <ComparisonChart
        title="接收消息趋势（条）"
        trend
        line
        unit="条"
        rows={buckets.map((b) => ({
          id: b.start,
          label: formatDate(b.start),
          axisLabel: bucketLabel(b.start, buckets),
          values: [b.received],
        }))}
        series={[{ id: 'received', label: '接收', color: '#1677ff' }]}
        onSelect={(id) => onSelect(id)}
      />
      <DataTable>
        <Table<Bucket>
          rowKey="start"
          size="small"
          dataSource={buckets}
          pagination={{ pageSize: 10 }}
          scroll={{ x: 760 }}
          columns={[
            {
              title: '周期起点（北京时间）',
              dataIndex: 'start',
              render: (v: string) => (
                <Button type="link" onClick={() => onSelect(v)}>
                  {formatDate(v)}
                </Button>
              ),
            },
            { title: '接收（条）', dataIndex: 'received', render: count },
            ...(['total', ...outcomes] as const).map((key) => ({
              title: key === 'total' ? '目标（项）' : outcomeLabels[key],
              render: (_: unknown, b: Bucket) => count(b.targets[key]),
            })),
          ]}
        />
      </DataTable>
    </section>
  )
}
export function OutcomeTrend({
  buckets,
  onSelect,
}: {
  buckets: Bucket[]
  onSelect: (bucket: string, outcome: Outcome) => void
}) {
  return (
    <section aria-label="目标结果趋势图">
      <ComparisonChart
        title="目标结果趋势（项）"
        trend
        stacked
        rows={buckets.map((b) => ({
          id: b.start,
          label: formatDate(b.start),
          axisLabel: bucketLabel(b.start, buckets),
          values: outcomes.map((key) => b.targets[key]),
        }))}
        series={resultSeries}
        onSelect={(id, series) => {
          if (isOutcome(series)) onSelect(id, series)
        }}
      />
      <DataTable>
        <Table<Bucket>
          rowKey="start"
          size="small"
          dataSource={buckets}
          pagination={{ pageSize: 10 }}
          scroll={{ x: 700 }}
          columns={[
            { title: '周期起点（北京时间）', dataIndex: 'start', render: formatDate },
            ...outcomes.map((key) => ({
              title: `${outcomeLabels[key]}（项）`,
              render: (_: unknown, b: Bucket) => (
                <Button
                  type="link"
                  disabled={b.targets[key] === '0'}
                  onClick={() => onSelect(b.start, key)}
                >
                  {count(b.targets[key])}
                </Button>
              ),
            })),
          ]}
        />
      </DataTable>
    </section>
  )
}
export function ResultChart({
  value,
  title,
  onSelect,
  attention = false,
}: {
  value: Counts
  title: string
  onSelect: (outcome: Outcome) => void
  attention?: boolean
}) {
  const keys = attention
    ? outcomes.filter((key) => key === 'unknown' || key === 'pending')
    : [...outcomes]
  return (
    <>
      {value.total === '0' ? (
        <Empty description="暂无目标或执行责任" />
      ) : (
        <ComparisonChart
          title={title}
          stacked={!attention}
          rows={
            attention
              ? keys.map((key) => ({ id: key, label: outcomeLabels[key], values: [value[key]] }))
              : [{ id: 'results', label: title, values: keys.map((key) => value[key]) }]
          }
          series={
            attention ? [{ id: 'attention', label: '当前关注', color: '#d48806' }] : resultSeries
          }
          onSelect={(id, series) => {
            const key = attention ? id : series
            if (isOutcome(key)) onSelect(key)
          }}
        />
      )}
      <Space wrap>
        {keys.map((key) => (
          <Button key={key} type="link" disabled={value[key] === '0'} onClick={() => onSelect(key)}>
            {outcomeLabels[key]} {count(value[key])}
          </Button>
        ))}
      </Space>
    </>
  )
}
export function MessageChart({
  summary,
  onSelect,
}: {
  summary: StatisticsOverview['summary']
  onSelect: (filter: DetailFilter) => void
}) {
  return (
    <>
      <ComparisonChart
        title="消息下发数量（条）"
        unit="条"
        rows={[
          { id: 'dispatched', label: '已形成下发', values: [summary.dispatched] },
          { id: 'not_dispatched', label: '未下发', values: [summary.not_dispatched] },
        ]}
        series={[{ id: 'messages', label: '来源消息', color: '#1677ff' }]}
        onSelect={(id) => {
          if (id === 'not_dispatched') onSelect({ object: 'message', outcome: 'not_dispatched' })
        }}
      />
      <Space wrap>
        <Button onClick={() => onSelect({ object: 'message' })}>
          接收 {count(summary.received)} 条
        </Button>
        <span>已形成下发 {count(summary.dispatched)} 条</span>
        <Button onClick={() => onSelect({ object: 'message', outcome: 'not_dispatched' })}>
          未下发 {count(summary.not_dispatched)} 条
        </Button>
      </Space>
      <p>
        部分通道未下发 {count(summary.partial_channel_failure)} 条；范围未完整{' '}
        {count(summary.incomplete)} 条；处理中消息 {count(summary.processing)}{' '}
        条。以上指标可能重叠。
      </p>
    </>
  )
}
export function GroupCharts({
  data,
  units = false,
  compact = false,
  onSelect,
}: {
  data: Breakdown[]
  units?: boolean
  compact?: boolean
  onSelect: (
    row: Breakdown,
    object: 'message' | 'target' | 'responsibility',
    outcome?: Outcome,
  ) => void
}) {
  const [metric, setMetric] = useState<GroupMetric>('targets')
  const rows = units ? rankGroups(data, metric) : data
  const percent = metric === 'allRate' || metric === 'settledRate'
  const object =
    metric === 'responsibilities' ? 'responsibility' : metric === 'received' ? 'message' : 'target'
  const title =
    metric === 'received'
      ? '接收量'
      : metric === 'responsibilities'
        ? '执行责任结果'
        : metric === 'allRate'
          ? '成功/全部目标'
          : metric === 'settledRate'
            ? '成功/明确结果'
            : '群目标结果'
  const name = (r: Breakdown) =>
    units
      ? `${r.platform_name ?? '未关联平台'} · ${r.name} [${r.id}]`
      : (channelNames[r.id] ?? r.name)
  return (
    <>
      {!compact && (
        <Select
          aria-label={units ? '单位比较指标' : '发送比较指标'}
          style={{ minWidth: 200 }}
          value={metric}
          onChange={setMetric}
          options={[
            { value: 'targets', label: '群目标结果（项）' },
            { value: 'responsibilities', label: '执行责任结果（项）' },
            ...(units ? [{ value: 'received', label: '接收量（条）' }] : []),
            { value: 'allRate', label: '成功/全部目标' },
            { value: 'settledRate', label: '成功/明确结果' },
          ]}
        />
      )}
      {units && (
        <p>
          按{title}排序，显示前 {rows.length} / 共 {data.length} 个业务单位。完整范围见下方数据表。
        </p>
      )}
      <ComparisonChart
        title={title}
        percent={percent}
        unit={metric === 'received' ? '条' : object === 'responsibility' ? '责任项' : '目标项'}
        stacked={!percent && metric !== 'received'}
        rows={rows.map((r) => ({
          id: r.id,
          label: name(r),
          values: percent
            ? [successRate(r.targets, metric === 'settledRate')?.toString() ?? null]
            : metric === 'received'
              ? [r.received]
              : outcomes.map(
                  (key) => r[metric === 'responsibilities' ? 'responsibilities' : 'targets'][key],
                ),
          descriptions: percent
            ? [
                `成功 ${count(r.targets.sent)} / 分母 ${count(metric === 'settledRate' ? String(BigInt(r.targets.sent) + BigInt(r.targets.failed)) : r.targets.total)}；失败 ${count(r.targets.failed)}，未知 ${count(r.targets.unknown)}，处理中 ${count(r.targets.pending)}`,
              ]
            : undefined,
        }))}
        series={
          percent || metric === 'received'
            ? [{ id: metric, label: title, color: '#1677ff' }]
            : resultSeries
        }
        onSelect={(id, series) => {
          const row = rows.find((r) => r.id === id)
          if (row) onSelect(row, object, isOutcome(series) ? series : undefined)
        }}
      />
      <DataTable>
        <Table<Breakdown>
          rowKey="id"
          dataSource={data}
          pagination={{ pageSize: 20 }}
          scroll={{ x: 1000 }}
          columns={[
            {
              title: '名称 / 标识',
              render: (_, r) => (
                <Button type="link" onClick={() => onSelect(r, object)}>
                  {name(r)}
                </Button>
              ),
            },
            { title: '接收/涉及来源消息（条）', dataIndex: 'received', render: count },
            { title: '群目标（项）', render: (_, r) => count(r.targets.total) },
            ...outcomes.map((key) => ({
              title: `目标${outcomeLabels[key]}`,
              render: (_: unknown, r: Breakdown) => (
                <Button
                  type="link"
                  disabled={r.targets[key] === '0'}
                  onClick={() => onSelect(r, 'target', key)}
                >
                  {count(r.targets[key])}
                </Button>
              ),
            })),
            {
              title: '成功/全部目标',
              render: (_, r) =>
                successRate(r.targets) == null ? '—' : `${successRate(r.targets)}%`,
            },
            {
              title: '成功/明确结果',
              render: (_, r) =>
                successRate(r.targets, true) == null ? '—' : `${successRate(r.targets, true)}%`,
            },
            { title: '责任总量（项）', render: (_, r) => count(r.responsibilities.total) },
            {
              title: '责任结果（项）',
              render: (_, r) => (
                <Space wrap>
                  {outcomes.map((key) => (
                    <Button
                      key={key}
                      type="link"
                      disabled={r.responsibilities[key] === '0'}
                      onClick={() => onSelect(r, 'responsibility', key)}
                    >
                      {outcomeLabels[key]} {count(r.responsibilities[key])}
                    </Button>
                  ))}
                </Space>
              ),
            },
          ]}
        />
      </DataTable>
    </>
  )
}
export function FailureCharts({
  failures,
  onSelect,
}: {
  failures: StatisticsOverview['failures']
  onSelect: (filter: DetailFilter) => void
}) {
  return (
    <>
      {(['message', 'target'] as const).map((object) => {
        const rows = failures
          .filter((row) => row.object === object)
          .sort((a, b) =>
            BigInt(a.count) === BigInt(b.count)
              ? a.reason_code.localeCompare(b.reason_code)
              : BigInt(a.count) > BigInt(b.count)
                ? -1
                : 1,
          )
          .slice(0, 10)
        return (
          <section key={object}>
            <h3>{object === 'message' ? '未下发消息原因（条）' : '失败与未知目标原因（项）'}</h3>
            <p>
              前 {rows.length} / 共 {failures.filter((row) => row.object === object).length}{' '}
              类原因，完整数据见原因表。
            </p>
            <ComparisonChart
              title={object === 'message' ? '未下发消息原因' : '失败与未知目标原因'}
              unit={object === 'message' ? '条' : '项'}
              rows={rows.map((r) => ({
                id: r.reason_code,
                label: receiptReasonLabels[r.reason_code] ?? r.reason_code,
                values: [r.count],
              }))}
              series={[
                { id: object, label: '数量', color: object === 'message' ? '#cf1322' : '#d48806' },
              ]}
              onSelect={(reason_code) =>
                onSelect({
                  object,
                  reason_code,
                  issues: object === 'target',
                  ...(object === 'message' ? { outcome: 'not_dispatched' } : {}),
                })
              }
            />
          </section>
        )
      })}
    </>
  )
}

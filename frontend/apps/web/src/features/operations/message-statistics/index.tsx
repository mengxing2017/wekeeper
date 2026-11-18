import { useMemo, useState } from 'react'
import { useQuery, useMutation } from '@tanstack/react-query'
import { useSearchParams } from 'react-router-dom'
import {
  Alert,
  Button,
  Card,
  Col,
  Drawer,
  Input,
  Row,
  Select,
  Space,
  Statistic,
  Table,
  Tabs,
  Tag,
  Typography,
  Skeleton,
} from 'antd'
import {
  messageStatistics,
  businessDirectory,
  receiptReasonLabels,
  type StatisticsFilter,
  type StatisticsOverview,
  type Counts,
  type Breakdown,
  type DetailFilter,
  type Detail,
  type Outcome,
} from './api'
import {
  Trend,
  OutcomeTrend,
  ResultChart,
  MessageChart,
  GroupCharts,
  FailureCharts,
  count,
} from './charts'

const filterKey = (value: StatisticsFilter) =>
  JSON.stringify(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)))
const filterValueNames: Record<string, string> = {
  hour: '小时',
  day: '日',
  week: '周',
  month: '月',
  year: '年',
  all: '全部来源',
  source_route: '来源自动分发',
  manual: '手动任务',
  wecom: '企业微信',
  mobile: '手机分发',
  wechat: '个人微信',
  unclassified: '未识别',
}
const titles: Record<string, string> = { day: '今日', week: '本周', month: '本月', year: '本年' }
const outcomeNames: Record<string, string> = {
  sent: '成功',
  failed: '失败',
  unknown: '未知',
  pending: '处理中',
  not_dispatched: '未下发',
  partial_failed: '部分失败',
}
const paths: Record<string, string> = {
  'wecom:wecom': '企业微信官方通道',
  'mobile:wecom': '手机企业微信',
  'mobile:wechat': '手机个人微信',
  'mobile:unclassified': '手机类型未识别',
}
const time = (v: string) => new Date(v).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })
function ratio(c: Counts, settled = false) {
  const n = BigInt(c.sent),
    d = settled ? n + BigInt(c.failed) : BigInt(c.total)
  return d === 0n ? '—' : `${Number((n * 10000n) / d) / 100}%`
}
function dateAt(offset = 0) {
  return new Date(Date.now() + 8 * 3600000 + offset * 86400000).toISOString().slice(0, 10)
}
function change(current: string, previous: string) {
  const c = BigInt(current),
    p = BigInt(previous)
  if (p === 0n) return c === 0n ? '无变化' : `新增 ${count(current)}`
  const points = ((c - p) * 10000n) / p
  const magnitude = points < 0n ? -points : points
  const fraction = String(magnitude % 100n)
    .padStart(2, '0')
    .replace(/0+$/, '')
  return `${c >= p ? '+' : '-'}${count(String(magnitude / 100n))}${fraction ? `.${fraction}` : ''}%`
}
function Details({
  snapshot,
  filter,
  onClose,
  onRefresh,
}: {
  snapshot: string
  filter: DetailFilter
  onClose: () => void
  onRefresh: () => void
}) {
  const [page, setPage] = useState(1)
  const q = useQuery({
    queryKey: ['statistics-details', snapshot, filter, page],
    queryFn: ({ signal }) => messageStatistics.details(snapshot, { ...filter, page }, signal),
    retry: false,
  })
  return (
    <Drawer title="统计明细" open size="large" onClose={onClose}>
      <Typography.Paragraph>
        与图表使用同一查询版本。原记录展示最新业务状态，可能较统计时间更新。
      </Typography.Paragraph>
      {q.error && (
        <Alert
          type="error"
          title={q.error.message}
          action={<Button onClick={onRefresh}>同步刷新统计</Button>}
        />
      )}
      <Table<Detail>
        rowKey="id"
        loading={q.isFetching}
        dataSource={q.error ? [] : (q.data?.items ?? [])}
        pagination={false}
        scroll={{ x: 950 }}
        columns={[
          { title: '归属时间', dataIndex: 'cohort_at', render: time },
          { title: '业务单位', dataIndex: 'unit_name' },
          { title: '目标', render: (_, r) => r.target_name ?? '来源消息' },
          {
            title: '发送途径',
            render: (_, r) =>
              r.channel ? (paths[`${r.channel}:${r.client_kind}`] ?? '类型未识别') : '—',
          },
          {
            title: '结果与原因',
            render: (_, r) => (
              <>
                {outcomeNames[r.outcome] ?? r.outcome}
                <div>{receiptReasonLabels[r.reason_code ?? ''] ?? r.reason_code}</div>
                {r.historical_risk && <Tag color="warning">历史未知风险</Tag>}
                {r.device_id && <div>责任手机：{r.device_id}</div>}
              </>
            ),
          },
          {
            title: '原记录',
            render: (_, r) =>
              r.receipt_id ? (
                <a
                  href={`/admin?view=source-automation-tasks&receipt=${encodeURIComponent(r.receipt_id)}`}
                >
                  查看消息
                </a>
              ) : r.task_id ? (
                <a
                  href={`/admin?view=${r.channel === 'mobile' ? 'mobile-execution-tasks' : 'broadcast-tasks'}&task=${encodeURIComponent(r.task_id)}`}
                >
                  查看任务
                </a>
              ) : (
                '尚未创建任务'
              ),
          },
        ]}
      />
      <Space>
        <Button disabled={page <= 1 || q.isFetching} onClick={() => setPage(page - 1)}>
          上一页
        </Button>
        <span>第 {page} 页</span>
        <Button
          disabled={!q.data?.has_more || q.isFetching || !!q.error}
          onClick={() => setPage(page + 1)}
        >
          下一页
        </Button>
      </Space>
    </Drawer>
  )
}
export default function StatisticsOverview() {
  const [params, setParams] = useSearchParams()
  const [storedDetail, setDetail] = useState<{
    snapshot: string
    filter: DetailFilter
    queryKey: string
  }>()
  const [pendingDetail, setPendingDetail] = useState<{
    channel: string
    client: string
    previous: string
    filter: DetailFilter
    queryKey: string
  }>()
  const filter = useMemo(() => {
    const f: Record<string, string> = {}
    for (const key of [
      'start',
      'end',
      'grain',
      'platform_id',
      'business_unit_id',
      'route_id',
      'source_kind',
      'channel',
      'client_kind',
    ]) {
      const v = params.get(`stats_${key}`)
      if (v) f[key] = v
    }
    return f as StatisticsFilter
  }, [params])
  const queryKey = filterKey(filter)
  const q = useQuery({
    queryKey: ['message-statistics', filter],
    queryFn: ({ signal }) => messageStatistics.create(filter, signal),
    retry: false,
    refetchInterval:
      storedDetail?.queryKey === queryKey || pendingDetail?.queryKey === queryKey ? false : 60000,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: false,
  })
  const platforms = useQuery({
    queryKey: ['statistics-platforms'],
    queryFn: async () => {
      const out = []
      for (let p = 1; ; p++) {
        const r = await businessDirectory.platforms(p, 100)
        out.push(...r.items)
        if (!r.has_more) return out
      }
    },
    retry: false,
  })
  const units = useQuery({
    queryKey: ['statistics-units', filter.platform_id],
    enabled: !!filter.platform_id,
    queryFn: async () => {
      const out = []
      for (let p = 1; ; p++) {
        const r = await businessDirectory.units(filter.platform_id!, p, 100)
        out.push(...r.items)
        if (!r.has_more) return out
      }
    },
    retry: false,
  })
  function set(key: string, value: string | undefined) {
    setDetail(undefined)
    setPendingDetail(undefined)
    setParams((previous) => {
      const next = new URLSearchParams(previous)
      if (value) next.set(`stats_${key}`, value)
      else next.delete(`stats_${key}`)
      if (key === 'platform_id') {
        next.delete('stats_business_unit_id')
        next.delete('stats_route_id')
      }
      return next
    })
  }
  function range(start: string, end: string) {
    setDetail(undefined)
    setPendingDetail(undefined)
    setParams((previous) => {
      const next = new URLSearchParams(previous)
      next.set('stats_start', start)
      next.set('stats_end', end)
      return next
    })
  }
  const data = q.data
  const pendingMatches =
    !!pendingDetail &&
    pendingDetail.queryKey === queryKey &&
    !!data &&
    !q.isFetching &&
    !q.error &&
    data.id !== pendingDetail.previous &&
    filter.channel === pendingDetail.channel &&
    filter.client_kind === pendingDetail.client &&
    data.filter.channel === pendingDetail.channel &&
    data.filter.client_kind === pendingDetail.client
  const detail =
    (storedDetail?.queryKey === queryKey ? storedDetail : undefined) ??
    (pendingMatches && pendingDetail && data
      ? { snapshot: data.id, filter: pendingDetail.filter }
      : undefined)
  function closeDetail() {
    setDetail(undefined)
    setPendingDetail(undefined)
  }
  function open(f: DetailFilter) {
    if (data && !q.error && !q.isFetching) {
      setPendingDetail(undefined)
      setDetail({ snapshot: data.id, filter: f, queryKey })
    }
  }
  const download = useMutation({
    mutationFn: () => messageStatistics.export(data!.id),
    onSuccess: (r) => {
      const url = URL.createObjectURL(new Blob([r.content], { type: 'text/csv;charset=utf-8' }))
      const a = document.createElement('a')
      a.href = url
      a.download = r.filename
      a.click()
      URL.revokeObjectURL(url)
    },
  })
  const tab = params.get('stats_tab') ?? 'overview'
  const received = (
    <>
      <Typography.Paragraph>
        接收按唯一收件计数，重复投递不增加。时间桶起点采用北京时间。
      </Typography.Paragraph>
      {data && (
        <Trend buckets={data.trend} onSelect={(bucket) => open({ object: 'message', bucket })} />
      )}
    </>
  )
  function selectChannel(
    row: Breakdown,
    object: 'message' | 'target' | 'responsibility' = 'target',
    outcome?: Outcome,
  ) {
    const [channel, client] = row.id.split(':')
    if (
      (channel !== 'wecom' && channel !== 'mobile') ||
      (client !== 'wecom' && client !== 'wechat' && client !== 'unclassified')
    )
      return
    if (!['wecom:wecom', 'mobile:wecom', 'mobile:wechat', 'mobile:unclassified'].includes(row.id))
      return
    closeDetail()
    if (outcome) {
      if (filter.channel === channel && filter.client_kind === client) {
        open({ object, outcome })
        return
      }
      if (data)
        setPendingDetail({
          channel,
          client,
          previous: data.id,
          filter: { object, outcome },
          queryKey: filterKey({ ...filter, channel, client_kind: client }),
        })
    }
    setParams((previous) => {
      const n = new URLSearchParams(previous)
      n.set('stats_channel', channel)
      n.set('stats_client_kind', client)
      n.set('stats_tab', 'sending')
      return n
    })
  }
  function content(d: StatisticsOverview) {
    switch (tab) {
      case 'receiving':
        return received
      case 'sending':
        return (
          <>
            <Alert type="info" title="同一来源消息可涉及多个发送途径，途径消息数不可直接相加" />
            <GroupCharts data={d.channels} onSelect={selectChannel} />
            <Button onClick={() => open({ object: 'responsibility' })}>查看独立执行责任</Button>
          </>
        )
      case 'units':
        return (
          <GroupCharts
            units
            data={d.units}
            onSelect={(row, object, outcome) => open({ object, unit: row.id, outcome })}
          />
        )
      case 'failures':
        return (
          <Space orientation="vertical" style={{ width: '100%' }}>
            <Space wrap>
              <Button onClick={() => open({ object: 'message', outcome: 'not_dispatched' })}>
                未下发消息 {count(d.summary.not_dispatched)} 条
              </Button>
              <Button onClick={() => open({ object: 'target', outcome: 'failed' })}>
                最终失败 {count(d.summary.targets.failed)} 项
              </Button>
              <Button onClick={() => open({ object: 'target', outcome: 'unknown' })}>
                结果未知 {count(d.summary.targets.unknown)} 项
              </Button>
            </Space>
            <Alert
              type="info"
              title={`历史未知风险涉及 ${count(d.summary.historical_risk)} 个目标，与当前结果桶有交集，不加入失败合计。`}
              description="未下发、最终失败和失败尝试是不同计数单位；超时或尚无反馈不等于确定未发送。"
            />
            <FailureCharts failures={d.failures} onSelect={open} />
            <Table
              rowKey={(r) => `${r.object}:${r.reason_code}`}
              dataSource={d.failures}
              pagination={{ pageSize: 20 }}
              columns={[
                {
                  title: '统计对象',
                  render: (_, r) =>
                    r.object === 'message' ? '未下发消息（条）' : '失败/未知目标（项）',
                },
                {
                  title: '原因',
                  render: (_, r) => receiptReasonLabels[r.reason_code] ?? r.reason_code,
                },
                { title: '数量', dataIndex: 'count', render: count },
                {
                  title: '明细',
                  render: (_, r) => (
                    <Button
                      onClick={() =>
                        open({
                          object: r.object,
                          reason_code: r.reason_code,
                          issues: r.object === 'target',
                          ...(r.object === 'message' ? { outcome: 'not_dispatched' as const } : {}),
                        })
                      }
                    >
                      查看明细
                    </Button>
                  ),
                },
              ]}
            />
          </Space>
        )
      default:
        return (
          <Space orientation="vertical" style={{ width: '100%' }}>
            <Row gutter={[16, 16]}>
              {d.cards.map((c) => (
                <Col key={c.key} xs={24} sm={12} xl={6}>
                  <Card title={`${titles[c.key]}接收（条）`}>
                    <Statistic value={c.count} formatter={() => count(c.count)} />
                    <Typography.Text>前期同进度 {count(c.previous)} 条；变化 {change(c.count, c.previous)}</Typography.Text>
                  </Card>
                </Col>
              ))}
            </Row>
            <Typography.Paragraph type="secondary">
              四张周期卡片跟随平台、业务单位、路由和来源方式；不受通道、客户端和主图日期范围影响。
            </Typography.Paragraph>
            <Card title="所选期间消息结果（条）">
              <MessageChart summary={d.summary} onSelect={open} />
            </Card>
            <Row gutter={[16, 16]}>
              <Col xs={24} lg={12}>
                <Card title="群目标结果（项）">
                  <p>
                    群目标 {count(d.summary.targets.total)} 项；成功/全部目标{' '}
                    {ratio(d.summary.targets)}，成功/明确结果 {ratio(d.summary.targets, true)}
                  </p>
                  <ResultChart
                    title="群目标结果"
                    value={d.summary.targets}
                    onSelect={(outcome) => open({ object: 'target', outcome })}
                  />
                </Card>
              </Col>
              <Col xs={24} lg={12}>
                <Card title="执行责任结果（项）">
                  <p>应执行责任 {count(d.summary.responsibilities.total)} 项</p>
                  <ResultChart
                    title="执行责任结果"
                    value={d.summary.responsibilities}
                    onSelect={(outcome) => open({ object: 'responsibility', outcome })}
                  />
                </Card>
              </Col>
            </Row>
            <Row gutter={[16, 16]}>
              <Col xs={24} xl={12}>
                <Card title="接收消息趋势（条）">{received}</Card>
              </Col>
              <Col xs={24} xl={12}>
                <Card title="目标结果趋势（项）">
                  <OutcomeTrend
                    buckets={d.trend}
                    onSelect={(bucket, outcome) => open({ object: 'target', bucket, outcome })}
                  />
                </Card>
              </Col>
            </Row>
            <Card title="发送途径对比（所选期间）">
              <GroupCharts compact data={d.channels} onSelect={selectChannel} />
            </Card>
            <Card title="当前关注（全部未结束目标，不限主图日期）">
              <p>共 {count(d.summary.attention.total)} 项</p>
              <ResultChart
                attention
                title="当前关注"
                value={d.summary.attention}
                onSelect={(outcome) => open({ object: 'target', outcome, attention: true })}
              />
            </Card>
          </Space>
        )
    }
  }
  return (
    <Space
      className="statistics-page"
      orientation="vertical"
      size="large"
      style={{ width: '100%', minWidth: 0 }}
    >
      <Space wrap>
        <label>
          开始日期{' '}
          <input
            aria-label="统计开始日期"
            type="date"
            max={dateAt()}
            value={filter.start ?? dateAt(-29)}
            onChange={(e) => set('start', e.target.value)}
          />
        </label>
        <label>
          结束日期{' '}
          <input
            aria-label="统计结束日期"
            type="date"
            max={dateAt()}
            value={filter.end ?? dateAt()}
            onChange={(e) => set('end', e.target.value)}
          />
        </label>
        <Select
          aria-label="统计粒度"
          value={filter.grain ?? 'day'}
          onChange={(v) => set('grain', v)}
          options={Object.entries({
            hour: '小时',
            day: '日',
            week: '周',
            month: '月',
            year: '年',
          }).map(([value, label]) => ({ value, label }))}
        />
        <Select
          aria-label="来源平台"
          allowClear
          placeholder="所有来源平台"
          style={{ minWidth: 160 }}
          value={filter.platform_id}
          onChange={(v) => set('platform_id', v)}
          options={platforms.data?.map((p) => ({ value: p.id, label: p.name }))}
        />
        <Select
          aria-label="业务单位"
          allowClear
          disabled={!filter.platform_id}
          placeholder="所有业务单位"
          style={{ minWidth: 160 }}
          value={filter.business_unit_id}
          onChange={(v) => set('business_unit_id', v)}
          options={units.data?.map((u) => ({ value: u.id, label: u.name }))}
        />

        <Select
          aria-label="来源方式"
          value={filter.source_kind ?? 'all'}
          onChange={(v) => set('source_kind', v)}
          options={[
            { value: 'all', label: '全部来源' },
            { value: 'source_route', label: '来源自动分发' },
            { value: 'manual', label: '手动任务' },
          ]}
        />
      </Space>
      <details>
        <summary>高级筛选</summary>
        <Space wrap>
          {' '}
          <Input.Search
            aria-label="路由筛选"
            placeholder="路由标识（可选）"
            style={{ width: 180 }}
            key={filter.route_id}
            defaultValue={filter.route_id}
            onSearch={(v) => set('route_id', v)}
          />{' '}
          <Select
            aria-label="发送通道"
            allowClear
            placeholder="所有通道"
            value={filter.channel}
            onChange={(v) => set('channel', v)}
            options={[
              { value: 'wecom', label: '企业微信官方' },
              { value: 'mobile', label: '手机分发' },
            ]}
          />
          <Select
            aria-label="客户端类型"
            allowClear
            placeholder="所有客户端"
            value={filter.client_kind}
            onChange={(v) => set('client_kind', v)}
            options={[
              { value: 'wecom', label: '企业微信' },
              { value: 'wechat', label: '个人微信' },
              { value: 'unclassified', label: '未识别' },
            ]}
          />
        </Space>
      </details>
      <Space wrap>
        {[
          ['今日', dateAt(), dateAt()],
          ['近 7 天', dateAt(-6), dateAt()],
          ['近 30 天', dateAt(-29), dateAt()],
          ['本月', `${dateAt().slice(0, 7)}-01`, dateAt()],
          ['本年', `${dateAt().slice(0, 4)}-01-01`, dateAt()],
        ].map(([label, start, end]) => (
          <Button key={label} onClick={() => range(start, end)}>
            {label}
          </Button>
        ))}
        <Button
          loading={q.isFetching}
          onClick={() => {
            closeDetail()
            void q.refetch()
          }}
        >
          刷新统计
        </Button>
        <Button
          disabled={!data || q.isFetching || !!q.error}
          loading={download.isPending}
          onClick={() => download.mutate()}
        >
          导出汇总 CSV
        </Button>
      </Space>
      <Space wrap>
        {Object.entries(filter).map(([key, value]) => (
          <Tag key={key} closable onClose={() => set(key, undefined)}>
            {
              (
                {
                  start: '开始日期',
                  end: '结束日期',
                  grain: '粒度',
                  platform_id: '来源平台',
                  business_unit_id: '业务单位',
                  route_id: '路由',
                  source_kind: '来源方式',
                  channel: '发送通道',
                  client_kind: '客户端',
                } as Record<string, string>
              )[key]
            }
            ：
            {key === 'platform_id'
              ? (platforms.data?.find((p) => p.id === value)?.name ?? value)
              : key === 'business_unit_id'
                ? (units.data?.find((u) => u.id === value)?.name ?? value)
                : (filterValueNames[value] ?? value)}
          </Tag>
        ))}
        <Button
          onClick={() => {
            closeDetail()
            setParams((previous) => {
              const next = new URLSearchParams(previous)
              for (const key of [...next.keys()])
                if (key.startsWith('stats_') && key !== 'stats_tab') next.delete(key)
              return next
            })
          }}
        >
          重置筛选
        </Button>
      </Space>
      {pendingDetail?.queryKey === queryKey && !detail && !q.error && (
        <p role="status">正在加载所选发送途径的统计明细…</p>
      )}

      {(platforms.error || units.error) && (
        <Alert
          type="warning"
          title="业务目录读取失败，统计仍可查询"
          description={(platforms.error ?? units.error)?.message}
        />
      )}
      {q.error && (
        <Alert
          type="error"
          title={`统计查询失败：${q.error.message}`}
          description="不会影响消息接收和发送，可通过导航继续使用业务页面。"
        />
      )}
      {download.error && <Alert type="error" title={`导出失败：${download.error.message}`} />}
      {q.isPending && (
        <div role="status">
          正在统计消息…
          <Skeleton active style={{ minHeight: 300 }} />
        </div>
      )}
      {data && !q.error && (
        <>
          <Typography.Text type="secondary">
            按接收/确认归属时间 · 北京时间（Asia/Shanghai）· 统计生成于 {time(data.generated_at)} ·{' '}
            {data.metric_version}。结果为查询时的持久事实；不代表群成员已读。
          </Typography.Text>
          {data.summary.incomplete !== '0' && (
            <Alert
              type="warning"
              title="部分消息目标范围尚未完整，当前目标结果不代表整条消息全部成功"
            />
          )}
          {data.unclassified !== '0' && (
            <Alert
              type="warning"
              title={`有 ${count(data.unclassified)} 项手机目标无法从冻结事实识别客户端类型，已单独列出。`}
            />
          )}
          <Tabs
            activeKey={tab}
            onChange={(v) => set('tab', v)}
            items={[
              ['overview', '概览'],
              ['receiving', '接收趋势'],
              ['sending', '发送分析'],
              ['failures', '失败与核对'],
              ['units', '业务单位'],
            ].map(([key, label]) => ({ key, label }))}
          />
          <div key={`${tab}:${JSON.stringify(filter)}`}>{content(data)}</div>
        </>
      )}
      {detail && (
        <Details
          key={JSON.stringify(detail)}
          snapshot={detail.snapshot}
          filter={detail.filter}
          onClose={closeDetail}
          onRefresh={() => {
            closeDetail()
            void q.refetch()
          }}
        />
      )}
    </Space>
  )
}

import { Empty } from 'antd'
import type { EChartsCoreOption } from './echarts'
import { EChart } from './echart'
import { countScale, formatCount } from './numbers'

export interface ChartRow {
  id: string
  label: string
  axisLabel?: string
  values: (string | null)[]
  descriptions?: string[]
}
export interface ChartSeries {
  id: string
  label: string
  color?: string
}
export function comparisonOption(
  rows: ChartRow[],
  series: ChartSeries[],
  { stacked = false, trend = false, line = false, percent = false, unit = '项' } = {},
): EChartsCoreOption {
  const totals = percent
    ? []
    : rows.map((row) =>
        String(row.values.reduce<bigint>((sum, value) => sum + BigInt(value ?? '0'), 0n)),
      )
  // Percentages are already fixed-point display values, not integer counts.
  const scale = countScale(
    percent
      ? []
      : [...rows.flatMap((row) => row.values.filter((v): v is string => v !== null)), ...totals],
  )
  const axis = {
    type: 'value',
    splitNumber: 3,
    axisLabel: { hideOverlap: true, ...(percent ? { formatter: '{value}%' } : {}) },
    min: 0,
    ...(percent ? { max: 100 } : { minInterval: scale.divisor === 1n ? 1 : undefined }),
    name: percent ? '%' : `${unit}${scale.label}`,
  }
  const category = {
    type: 'category',
    data: rows.map((row) => row.label),
    axisLabel: {
      hideOverlap: true,
      width: trend ? 85 : 125,
      overflow: 'truncate',
      formatter: (value: string, index: number) => rows[index]?.axisLabel ?? value,
    },
    ...(!trend ? { inverse: true } : {}),
  }
  return {
    animation: false,
    aria: {
      enabled: true,
      decal: { show: true },
      label: { description: '精确数值与键盘操作请使用图表后的数据表。' },
    },
    grid: {
      top: 60,
      left: trend ? 50 : rows.length === 1 ? 90 : 125,
      right: 25,
      bottom: trend && rows.length > 24 ? 95 : 55,
      outerBoundsMode: 'same',
      containLabel: false,
    },
    legend: { type: 'scroll', top: 0, data: series.map((s) => s.label) },
    tooltip: {
      trigger: 'axis',
      confine: true,
      renderMode: 'richText',
      formatter: (params: unknown) => {
        const items = (Array.isArray(params) ? params : [params]) as {
          data?: { raw?: string | null; description?: string }
          name?: string
          seriesName?: string
        }[]
        return [
          items[0]?.name,
          ...items.map(
            (p) =>
              `${p.seriesName}：${p.data?.raw == null ? '—' : percent ? `${p.data.raw}%` : `${formatCount(p.data.raw)} ${unit}`}${p.data?.description ? `\n${p.data.description}` : ''}`,
          ),
        ].join('\n')
      },
    },
    xAxis: trend ? category : axis,
    yAxis: trend ? axis : category,
    ...(trend && rows.length > 24
      ? {
          dataZoom: [
            { type: 'slider', bottom: 8, start: 0, end: 100 },
            { type: 'inside', zoomOnMouseWheel: false, moveOnMouseMove: false },
          ],
        }
      : {}),
    series: series.map((s, i) => ({
      id: s.id,
      name: s.label,
      type: line ? 'line' : 'bar',
      ...(stacked ? { stack: 'results' } : {}),
      smooth: false,
      showSymbol: rows.length < 80,
      symbolSize: 7,
      barMaxWidth: 34,
      itemStyle: { color: s.color },
      data: rows.map((row) => ({
        id: row.id,
        raw: row.values[i],
        description: row.descriptions?.[i],
        value:
          row.values[i] == null
            ? null
            : percent
              ? Number(row.values[i])
              : scale.project(row.values[i]!),
        name: row.label,
      })),
    })),
  }
}
export function ComparisonChart({
  rows,
  series,
  title,
  stacked = false,
  trend = false,
  line = false,
  percent = false,
  unit = '项',
  onSelect,
}: {
  rows: ChartRow[]
  series: ChartSeries[]
  title: string
  stacked?: boolean
  trend?: boolean
  line?: boolean
  percent?: boolean
  unit?: string
  onSelect?: (id: string, series: string) => void
}) {
  if (!rows.length) return <Empty description="暂无统计数据" />
  return (
    <EChart
      label={title}
      height={trend ? 320 : Math.max(260, Math.min(620, rows.length * 44 + 110))}
      zoom={trend && rows.length > 24}
      option={comparisonOption(rows, series, { stacked, trend, line, percent, unit })}
      onSelect={onSelect}
    />
  )
}

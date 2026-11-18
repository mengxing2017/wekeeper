import type { Breakdown, Counts, Outcome } from './api'
import { percentage } from '../../../shared/charts/numbers'
export const outcomes = ['sent', 'failed', 'unknown', 'pending'] as const
export type { Outcome } from './api'
export const outcomeLabels = { sent: '成功', failed: '失败', unknown: '未知', pending: '处理中' }
export const resultSeries = outcomes.map((id, i) => ({
  id,
  label: outcomeLabels[id],
  color: ['#389e0d', '#cf1322', '#d48806', '#1677ff'][i],
}))
export const channelNames: Record<string, string> = {
  'wecom:wecom': '企业微信官方通道',
  'mobile:wecom': '手机企业微信',
  'mobile:wechat': '手机个人微信',
  'mobile:unclassified': '手机类型未识别',
}
export type GroupMetric = 'targets' | 'responsibilities' | 'received' | 'allRate' | 'settledRate'
export function successRate(c: Counts, settled = false) {
  return percentage(c.sent, settled ? String(BigInt(c.sent) + BigInt(c.failed)) : c.total)
}
export function rankGroups(rows: Breakdown[], metric: GroupMetric, limit = 10) {
  const value = (r: Breakdown) =>
    metric === 'received'
      ? BigInt(r.received)
      : BigInt(r[metric === 'responsibilities' ? 'responsibilities' : 'targets'].total)
  return [...rows]
    .sort((a, b) => {
      if (metric === 'allRate' || metric === 'settledRate') {
        const denominator = (r: Breakdown) =>
          metric === 'allRate'
            ? BigInt(r.targets.total)
            : BigInt(r.targets.sent) + BigInt(r.targets.failed)
        const da = denominator(a),
          db = denominator(b)
        if (da === 0n || db === 0n) return da === db ? a.id.localeCompare(b.id) : da === 0n ? 1 : -1
        const diff = BigInt(a.targets.sent) * db - BigInt(b.targets.sent) * da
        return diff === 0n ? a.id.localeCompare(b.id) : diff > 0n ? -1 : 1
      }
      return value(a) === value(b) ? a.id.localeCompare(b.id) : value(a) > value(b) ? -1 : 1
    })
    .slice(0, limit)
}
export const isOutcome = (value: string): value is Outcome => outcomes.some((key) => key === value)

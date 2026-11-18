import { expect, it } from 'vitest'
import { countScale, formatCount, percentage } from '../../../shared/charts/numbers'
import { rankGroups, successRate } from './chart-data'
import type { Breakdown } from './api'
const counts = { total: '10', sent: '3', failed: '2', unknown: '4', pending: '1' }
const row = (id: string, total: string): Breakdown => ({
  id,
  name: '同名单位',
  platform_name: '平台',
  received: total,
  targets: { ...counts, total },
  responsibilities: { ...counts, total: '20' },
})
it('keeps exact counts and ratios while projecting huge geometry on one scale', () => {
  const raw = '900719925474099312345'
  const scale = countScale([raw, '2'])
  expect(scale.divisor).toBeGreaterThan(1n)
  expect(scale.project(raw)).toBeLessThanOrEqual(Number.MAX_SAFE_INTEGER)
  expect(scale.project('2')).toBeGreaterThan(0)
  expect(formatCount(raw)).toBe('900,719,925,474,099,312,345')
  expect(percentage('1', '3')).toBe(33.33)
  expect(successRate(counts)).toBe(30)
  expect(successRate(counts, true)).toBe(60)
  expect(percentage('0', '0')).toBeNull()
})
it('ranks the complete unit collection by exact values with stable identity and no mutation', () => {
  const rows = Array.from({ length: 35 }, (_, i) => row(`unit-${i}`, String(i)))
  rows.push(row('big-b', '9007199254740993'), row('big-a', '9007199254740993'))
  const before = [...rows]
  const ranked = rankGroups(rows, 'targets')
  expect(ranked).toHaveLength(10)
  expect(ranked.slice(0, 3).map((r) => r.id)).toEqual(['big-a', 'big-b', 'unit-34'])
  expect(rows).toEqual(before)
  const empty = row('empty', '0')
  expect(rankGroups([empty, row('valid', '10')], 'allRate')[0].id).toBe('valid')
})

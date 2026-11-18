import { expect, it } from 'vitest'
import { comparisonOption } from './comparison'
it('preserves exact tooltip values and identities even when huge stacked values need projection', () => {
  const option = comparisonOption(
    [{ id: 'unit-real-id', label: '单位', values: ['9007199254740993123', '2'] }],
    [
      { id: 'sent', label: '成功' },
      { id: 'pending', label: '处理中' },
    ],
    { stacked: true },
  )
  const series = option.series as { data: { id: string; raw: string; value: number }[] }[]
  expect(series[0].data[0].id).toBe('unit-real-id')
  expect(series[0].data[0].raw).toBe('9007199254740993123')
  expect(series[0].data[0].value).toBeLessThan(Number.MAX_SAFE_INTEGER)
  expect(series[1].data[0].value).toBeGreaterThan(0)
  const tooltip = option.tooltip as { formatter: (params: unknown) => string }
  expect(
    tooltip.formatter([{ name: '单位', seriesName: '成功', data: series[0].data[0] }]),
  ).toContain('9,007,199,254,740,993,123')
})
it('supports fractional percentages and treats missing denominators separately from zero percent', () => {
  const option = comparisonOption(
    [
      { id: 'missing', label: '无分母', values: [null] },
      { id: 'zero', label: '零成功', values: ['0'] },
      { id: 'ratio', label: '比率', values: ['33.33'] },
    ],
    [{ id: 'allRate', label: '成功率' }],
    { percent: true },
  )
  const series = option.series as { data: { value: number | null }[] }[]
  expect(series[0].data.map((point) => point.value)).toEqual([null, 0, 33.33])
  expect(option.xAxis).toMatchObject({ min: 0, max: 100 })
})

import { expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import MemberChart from './member-chart'
import { ComparisonChart } from '../../../shared/charts/comparison'
vi.mock('../../../shared/charts/comparison', () => ({ ComparisonChart: vi.fn(() => null) }))
it('compares only supplied page members and keeps missing counts distinct from zero', () => {
  render(
    <MemberChart
      rows={[
        {
          id: 'page-2-member',
          name: '成员',
          owned_group_count: 0,
          participating_group_count: null,
        },
      ]}
    />,
  )
  expect(screen.getByRole('heading', { name: '当前页成员比较' })).toBeTruthy()
  expect(ComparisonChart).toHaveBeenCalledWith(
    expect.objectContaining({
      rows: [{ id: 'page-2-member', label: '成员', values: ['0', null] }],
    }),
    undefined,
  )
})

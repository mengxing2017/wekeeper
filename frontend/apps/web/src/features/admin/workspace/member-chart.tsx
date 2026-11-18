import { ComparisonChart } from '../../../shared/charts/comparison'
import type { Row } from './api'
export default function MemberChart({ rows }: { rows: Row[] }) {
  const value = (v: Row[string]) =>
    typeof v === 'number' && Number.isSafeInteger(v) && v >= 0
      ? String(v)
      : typeof v === 'string' && /^\d+$/.test(v)
        ? v
        : null
  return (
    <section aria-label="当前页成员比较">
      <h3>当前页成员比较</h3>
      <p>仅比较当前页，非全局排名。群主群与参与群可重叠，两种关系不相加；精确数值见下方成员表。</p>
      <ComparisonChart
        title="当前页成员群关系（个）"
        unit="个"
        rows={rows.map((r) => ({
          id: r.id,
          label: `${r.name ?? r.userid ?? r.id}`,
          values: [value(r.owned_group_count), value(r.participating_group_count)],
        }))}
        series={[
          { id: 'owned', label: '群主群', color: '#1677ff' },
          { id: 'participating', label: '参与群', color: '#722ed1' },
        ]}
      />
    </section>
  )
}

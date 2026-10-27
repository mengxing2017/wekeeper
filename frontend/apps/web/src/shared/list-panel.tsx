import type { ReactNode } from 'react'
import { Card } from 'antd'

/** ProTable's query card, table toolbar and pagination arrangement. */
export function ListPanel({
  title = '查询结果',
  filters,
  actions,
  footer,
  children,
}: {
  title?: ReactNode
  filters?: ReactNode
  actions?: ReactNode
  footer?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="list-page">
      {filters && <div className="query-filter">{filters}</div>}
      <Card className="data-panel" title={title} extra={actions}>
        {children}
        {footer}
      </Card>
    </section>
  )
}

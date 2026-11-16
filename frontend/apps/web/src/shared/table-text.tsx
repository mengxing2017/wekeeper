import { Typography } from 'antd'

/** Native disclosure keeps full text available to keyboard and touch users. */
export function TableText({ value, limit = 36, label = '内容' }: {
  value: unknown
  limit?: number
  label?: string
}) {
  const text = value == null || value === '' ? '—' : String(value)
  const characters = Array.from(text)
  if (characters.length <= limit) return <span className="table-text">{text}</span>
  return <details className="table-text table-text-disclosure">
    <summary aria-label={`查看完整${label}`}>{characters.slice(0, limit).join('')}…</summary>
    <div>{text}</div>
  </details>
}

export function TableIdentifier({ value }: { value: string | null | undefined }) {
  if (!value) return <span>—</span>
  return <div className="table-identifier">
    <TableText value={value} limit={24} label="标识" />
    <Typography.Text copyable={{ text: value }} />
  </div>
}

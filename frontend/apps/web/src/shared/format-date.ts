/** Match the reference frontend: localized date/time in the browser's timezone. */
export function parseDate(value: unknown): Date | undefined {
  if (typeof value !== 'string' || !value.trim()) return undefined
  const date = new Date(value.replace(/(T\d{2}:\d{2}:\d{2}(?:\.\d+)?[+-]\d{2})$/, '$1:00'))
  return Number.isNaN(date.getTime()) ? undefined : date
}

export function formatDate(value: unknown, empty = '-'): string {
  return parseDate(value)?.toLocaleString('zh-CN') ?? empty
}

/** WeCom group membership timestamps are Unix seconds, not milliseconds. */
export function formatUnixSeconds(value: unknown): string {
  if (typeof value !== 'number' || !Number.isFinite(value)) return '-'
  const date = new Date(value * 1000)
  return Number.isNaN(date.getTime()) ? '-' : date.toLocaleString('zh-CN')
}

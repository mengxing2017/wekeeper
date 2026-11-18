// Counts stay exact outside the geometry projection used by the renderer.
export const formatCount = (value: string) => BigInt(value).toLocaleString('zh-CN')
export function countScale(values: string[]) {
  const maximum = values.reduce((max, value) => (BigInt(value) > max ? BigInt(value) : max), 0n)
  let divisor = 1n
  while (maximum > BigInt(Number.MAX_SAFE_INTEGER) * divisor) divisor *= 10n
  return {
    divisor,
    label: divisor === 1n ? '' : `（单位 ×${formatCount(String(divisor))}，图形近似）`,
    project: (value: string) =>
      Number(BigInt(value) / divisor) + Number(BigInt(value) % divisor) / Number(divisor),
  }
}
export function percentage(sent: string, denominator: string): number | null {
  const total = BigInt(denominator)
  return total === 0n ? null : Number((BigInt(sent) * 10000n) / total) / 100
}

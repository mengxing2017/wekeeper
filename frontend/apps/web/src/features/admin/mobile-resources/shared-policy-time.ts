/** ISO weekday values match PostgreSQL isodow; labels are for administrators. */
export const policyWeekdays = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'].map((label, i) => ({ label, value: i + 1 }))
export function policyClock(minute: number | undefined): string {
  if (minute === undefined || !Number.isInteger(minute) || minute < 0 || minute > 1440) return ''
  const value = minute % 1440
  return `${String(Math.floor(value / 60)).padStart(2, '0')}:${String(value % 60).padStart(2, '0')}`
}
export function policyMinute(clock: string, end: boolean): number | undefined {
  if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(clock)) return undefined
  const [hour, minute] = clock.split(':').map(Number)
  const value = hour * 60 + minute
  return end && value === 0 ? 1440 : value
}
export const newPolicyWindow = () => ({ weekdays: [1, 2, 3, 4, 5], start_minute: 540, end_minute: 1080 })

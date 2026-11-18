import { expect, it } from 'vitest'
import { attemptFailureReason } from './task-model'
it('preserves safe diagnostic codes for known, unknown and missing reasons', () => {
  expect(attemptFailureReason('device_unavailable')).toContain('本次暂不可执行')
  expect(attemptFailureReason('device_unavailable')).toContain('原因码：device_unavailable')
  expect(attemptFailureReason('future_reason')).toContain('原因码：future_reason')
  expect(attemptFailureReason('constructor')).toContain('手机未完成本次执行')
  expect(attemptFailureReason(null)).toBe('未提供具体原因')
  expect(attemptFailureReason(' \n\u202e ')).toBe('未提供具体原因')
  expect(attemptFailureReason('a'.repeat(300))).not.toContain('a'.repeat(201))
  expect(attemptFailureReason('receipt_timeout')).toContain('60 秒')
  expect(attemptFailureReason('result_timeout')).toContain('5 分钟')
})

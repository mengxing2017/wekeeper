import { expect, test } from 'vitest'
import { canCancelTask, canConfirmPreview, messageProblem, taskCounts, taskError } from './task-model'
import type { ExecutionTaskDetail } from './api'
test('body byte budget matches server and preserves intentional whitespace', () => {
  expect(messageProblem(' 原文\n正文\t ')).toBeUndefined()
  expect(messageProblem('中'.repeat(1333) + 'a')).toBeUndefined()
  expect(messageProblem('中'.repeat(1333) + 'aa')).toContain('4000')
  expect(messageProblem(' \n\t')).toBeTruthy()
  expect(messageProblem('正文\0')).toBeTruthy()
})
test('unknown and issued calls never appear cancellable or delivered', () => {
  const task = { state: 'queued', items: [{ state: 'pending' }] } as ExecutionTaskDetail
  expect(canCancelTask(task)).toBe(true)
  for (const state of ['claimed', 'prepared', 'invocation_issued', 'unknown', 'reported_sent'] as const) {
    task.items[0].state = state
    expect(canCancelTask(task)).toBe(false)
  }
  task.items = [{ state: 'unknown' }, { state: 'reported_sent' }, { state: 'invocation_issued' }] as ExecutionTaskDetail['items']
  expect(taskCounts(task)).toBe('报告已发 1；确定未发/失败 0；未执行 0；等待回报 1；待核对 1')
})
test('known task blockers give concrete next steps without leaking foreign school details', () => {
  expect(taskError({ code: 'mobile_device_reserved' })).toContain('平台管理员协调')
  expect(taskError({ code: 'mobile_target_unresolved' })).toContain('不要换设备')
  expect(taskError({ code: 'mobile_preview_expired' })).toContain('重新预览')
  expect(taskError({ code: 'revision_conflict' })).toContain('不能沿用旧确认')
  expect(taskError(new Error('网络中断'))).toBe('网络中断')
})

test('expiry blocks new confirmation but permits retrieving an attempted confirmation receipt', () => {
  const expires = '2026-11-01T01:00:00Z'
  expect(canConfirmPreview(expires, Date.parse(expires) - 1, false)).toBe(true)
  expect(canConfirmPreview(expires, Date.parse(expires), false)).toBe(false)
  expect(canConfirmPreview(expires, Date.parse(expires) + 1000, true)).toBe(true)
  expect(canConfirmPreview('invalid', Date.now(), false)).toBe(false)
})

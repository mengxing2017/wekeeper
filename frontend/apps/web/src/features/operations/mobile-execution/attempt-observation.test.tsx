import { render, screen } from '@testing-library/react'
import { expect, it } from 'vitest'
import type { SharedAttempt } from './api'
import { AttemptObservation } from './attempt-observation'
it('does not turn a later inbox receipt into an execution receipt', () => {
  const attempt = { id: 'attempt-A', execution_id: 'execution-A', inbox_received_at: '2026-11-17T17:41:50Z', result_received_at: '2026-11-17T17:41:38Z' } as SharedAttempt
  render(<AttemptObservation attempt={attempt}/>)
  expect(screen.getByText(/任务列表同步回执（平台记录）/)).toBeTruthy()
  expect(screen.getByText(/执行结果收报（平台记录）/)).toBeTruthy()
  expect(screen.getByText(/结果已收到，未单独记录落盘确认/)).toBeTruthy()
  expect(screen.getByText(/UTC\+08:00/)).toBeTruthy()
  expect(screen.queryByText(/执行包落盘确认（平台记录）/)).toBeNull()
  expect(screen.getByText(/execution-A/)).toBeTruthy()
})

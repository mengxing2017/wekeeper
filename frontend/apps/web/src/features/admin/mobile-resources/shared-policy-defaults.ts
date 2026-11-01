import type { SharedPolicyInput } from './api'

/** Editable starting values; existing saved policies always take precedence. */
export function newSharedPolicyLimits(): SharedPolicyInput['limits'] {
  return {
    global: { interval_seconds: 5, window_seconds: 3600, max_calls: 120, daily_max_calls: 1000 },
    school: { interval_seconds: 10, window_seconds: 3600, max_calls: 60, daily_max_calls: 300 },
    device: { interval_seconds: 30, window_seconds: 3600, max_calls: 30, daily_max_calls: 150 },
    target: { interval_seconds: 60, window_seconds: 3600, max_calls: 10, daily_max_calls: 30 },
  }
}

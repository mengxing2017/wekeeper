import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { DeviceControlPanel } from './device-control'
import { api } from './api'
vi.mock('./api', () => ({ api: { controlPresence: vi.fn(), controlCommand: vi.fn() } }))
afterEach(() => { cleanup(); vi.resetAllMocks() })
it('shows control presence independently of execution readiness and queues a platform pause', async () => {
  vi.mocked(api.controlPresence).mockResolvedValue({ id: 'phone', state: 'online', transport: 'wss', last_control_heartbeat_at: '2026-10-31T00:00:00Z', readiness: 'blocked', block_reason: 'device_locked', desired_paused: false, desired_revision: 0, credential_expires_at: null, commands: [] })
  vi.mocked(api.controlCommand).mockResolvedValue({ id: 'command', kind: 'pause_execution', state: 'queued', sequence: 1, expires_at: '2026-11-01T00:00:00Z', result_code: null })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><DeviceControlPanel deviceId="phone" /></QueryClientProvider>)
  expect(await screen.findByText('在线')).toBeTruthy()
  expect(screen.getByText(/等待手机解锁/)).toBeTruthy()
  expect(screen.getByText(/解除平台暂停不会解除手机紧急停止/)).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '暂停执行' }))
  await waitFor(() => expect(api.controlCommand).toHaveBeenCalledExactlyOnceWith('phone', 'pause_execution', expect.any(String)))
  await waitFor(() => expect(api.controlPresence).toHaveBeenCalledTimes(2))
  client.clear()
})

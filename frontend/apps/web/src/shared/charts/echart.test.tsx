import { StrictMode } from 'react'
import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { EChart } from './echart'
const { init, instances } = vi.hoisted(() => {
  const instances: {
    setOption: ReturnType<typeof vi.fn>
    resize: ReturnType<typeof vi.fn>
    dispose: ReturnType<typeof vi.fn>
    on: ReturnType<typeof vi.fn>
    off: ReturnType<typeof vi.fn>
  }[] = []
  return {
    instances,
    init: vi.fn(() => {
      const instance = {
        setOption: vi.fn(),
        resize: vi.fn(),
        dispose: vi.fn(),
        on: vi.fn(),
        off: vi.fn(),
      }
      instances.push(instance)
      return instance
    }),
  }
})
vi.mock('./echarts', () => ({ init }))
afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  instances.length = 0
})
it('disposes StrictMode instances and listeners and uses the current click callback', async () => {
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(500)
  const first = vi.fn(),
    next = vi.fn()
  const view = render(
    <StrictMode>
      <EChart label="趋势" option={{ series: [] }} onSelect={first} />
    </StrictMode>,
  )
  await waitFor(() => expect(instances.length).toBeGreaterThan(0))
  const chart = instances.at(-1)!
  view.rerender(
    <StrictMode>
      <EChart label="趋势" option={{ series: [{ id: 'new' }] }} onSelect={next} />
    </StrictMode>,
  )
  const click = chart.on.mock.calls.find((call) => call[0] === 'click')![1]
  click({ data: { id: 'real-unit-id', raw: '1' }, seriesId: 'sent' })
  expect(first).not.toHaveBeenCalled()
  expect(next).toHaveBeenCalledWith('real-unit-id', 'sent')
  view.unmount()
  expect(chart.off).toHaveBeenCalledWith('click')
  expect(chart.dispose).toHaveBeenCalledTimes(1)
})
it('keeps a readable fallback when renderer initialization fails', async () => {
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(500)
  init.mockImplementationOnce(() => {
    throw new Error('renderer unavailable')
  })
  render(<EChart label="趋势" option={{}} />)
  expect(await screen.findByText('图表暂不可用，请使用数据表核对')).toBeTruthy()
})

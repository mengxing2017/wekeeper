import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { Button } from 'antd'
import { useDialogs } from './use-dialogs'

const originalComputedStyle = window.getComputedStyle
window.getComputedStyle = element => originalComputedStyle(element)
afterEach(cleanup)
function DialogExample({ result }: { result: (value: string | null) => void }) {
  const dialogs = useDialogs()
  return <>{dialogs.contextHolder}<Button onClick={async () => result(await dialogs.prompt('审核说明'))}>审核内容</Button></>
}
it('does not accept a cancelled prompt', async () => {
  const result = vi.fn()
  render(<DialogExample result={result} />)
  fireEvent.click(screen.getByText('审核内容'))
  fireEvent.change(await screen.findByRole('textbox', { name: '审核说明' }), { target: { value: '未确认说明' } })
  fireEvent.click(screen.getByRole('button', { name: /^取\s*消$/ }))
  await waitFor(() => expect(result).toHaveBeenCalledWith(null))
})
it('returns the entered reason only after confirmation', async () => {
  const result = vi.fn()
  render(<DialogExample result={result} />)
  fireEvent.click(screen.getByText('审核内容'))
  fireEvent.change(await screen.findByRole('textbox', { name: '审核说明' }), { target: { value: '已核对' } })
  expect(result).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: /^确\s*认$/ }))
  await waitFor(() => expect(result).toHaveBeenCalledWith('已核对'))
})

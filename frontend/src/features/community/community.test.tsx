import type { ReactNode } from 'react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import Community from './index'
import { workspace } from './api'

vi.mock('../admin/workspace', () => ({
  SessionGate: ({ children }: { children: (user: unknown) => ReactNode }) => children({ id: 'u', display_name: '用户', bound_school_id: 's' }),
  label: (value: unknown) => String(value ?? ''),
}))
vi.mock('./api', () => ({ identity: { profile: vi.fn() }, workspace: {
  schools: vi.fn(), directory: vi.fn(), list: vi.fn(), mutate: vi.fn(), upload: vi.fn(),
} }))
beforeEach(() => {
  vi.mocked(workspace.schools).mockResolvedValue([{ id: 's', name: '测试学校', active: true, revision: 1, config: {} }])
  vi.mocked(workspace.directory).mockResolvedValue([])
  vi.mocked(workspace.list).mockResolvedValue({ items: [], page: { number: 1, size: 20, has_more: false } })
  vi.mocked(workspace.mutate).mockResolvedValue({})
  vi.mocked(workspace.upload).mockResolvedValue({ url: '/image.png' })
})
afterEach(() => { cleanup(); vi.resetAllMocks() })
function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><MemoryRouter><Community /></MemoryRouter></QueryClientProvider>)
}
it('uploads through the contract and submits managed form values with image URLs', async () => {
  show()
  fireEvent.click(await screen.findByRole('button', { name: /^发\s*布$/ }))
  fireEvent.change(screen.getByLabelText('标题'), { target: { value: '校园通知' } })
  fireEvent.change(screen.getByLabelText('正文'), { target: { value: '通知正文' } })
  fireEvent.change(screen.getByLabelText('标签（逗号分隔）'), { target: { value: '校园, 通知' } })
  const file = new File(['image'], 'photo.png', { type: 'image/png' })
  fireEvent.change(document.querySelector('input[type=file]')!, { target: { files: [file] } })
  await waitFor(() => expect(workspace.upload).toHaveBeenCalledWith('s', expect.objectContaining({ name: 'photo.png' })))
  await screen.findByAltText('上传预览')
  fireEvent.click(screen.getByRole('button', { name: '提交审核' }))
  await waitFor(() => expect(workspace.mutate).toHaveBeenCalledWith('s', 'posts', expect.objectContaining({
    title: '校园通知', content: '通知正文', tags: ['校园', '通知'], images: ['/image.png'],
  }), 'POST'))
})
it('validates feedback before sending and preserves an empty optional contact', async () => {
  show()
  fireEvent.click(await screen.findByRole('button', { name: /^反\s*馈$/ }))
  fireEvent.click(screen.getByRole('button', { name: '提交反馈' }))
  await screen.findByText('请填写意见反馈')
  expect(workspace.mutate).not.toHaveBeenCalled()
  fireEvent.change(screen.getByLabelText('意见反馈'), { target: { value: '建议内容' } })
  fireEvent.click(screen.getByRole('button', { name: '提交反馈' }))
  await waitFor(() => expect(workspace.mutate).toHaveBeenCalledWith('s', 'cases', { kind: 'feedback', content: '建议内容', contact: '' }))
})

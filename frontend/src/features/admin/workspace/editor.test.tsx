import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { Editor } from './editor'
import { selectOption } from '../../../test/select-option'

afterEach(cleanup)

it('validates required selections before saving and preserves the string payload', async () => {
  const save = vi.fn().mockResolvedValue(undefined)
  render(<Editor fields={[
    { name: 'name', title: '模板名称', value: '通知' },
    { name: 'state', title: '状态', options: [{ value: 'active', label: '启用' }] },
    { name: 'sort', title: '排序', type: 'number', value: 3 },
    { name: 'note', title: '备注', type: 'textarea', optional: true },
  ]} onSave={save} />)
  fireEvent.click(screen.getByRole('button', { name: /^保\s*存$/ }))
  expect(await screen.findByText('请填写状态')).toBeTruthy()
  expect(save).not.toHaveBeenCalled()
  await selectOption(screen.getByLabelText('状态'), '启用')
  fireEvent.click(screen.getByRole('button', { name: /^保\s*存$/ }))
  await waitFor(() => expect(save).toHaveBeenCalledWith({ name: '通知', state: 'active', sort: '3', note: '' }))
  expect(await screen.findByRole('status')).toBeTruthy()
})

it('keeps a read-only identity in the payload and allows retry after an error', async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error('版本冲突')).mockResolvedValue(undefined)
  render(<Editor fields={[{ name: 'id', title: '身份', value: 'fixed', readOnly: true }]} onSave={save} />)
  expect((screen.getByLabelText('身份') as HTMLInputElement).readOnly).toBe(true)
  fireEvent.click(screen.getByRole('button', { name: /^保\s*存$/ }))
  expect(await screen.findByText('版本冲突')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: /^保\s*存$/ }))
  await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
  expect(save).toHaveBeenLastCalledWith({ id: 'fixed' })
  expect(await screen.findByRole('status')).toBeTruthy()
})

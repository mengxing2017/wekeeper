import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { AppShell } from './index'

afterEach(cleanup)
function show(platformAdmin = false) {
  const onNavigate = vi.fn()
  const onLogout = vi.fn().mockResolvedValue(undefined)
  render(<MemoryRouter><AppShell active="posts" onNavigate={onNavigate} username="测试运营" platformAdmin={platformAdmin} onLogout={onLogout}><p>学校内容</p></AppShell></MemoryRouter>)
  return { onNavigate, onLogout }
}
it('shows role-appropriate navigation and routes business selections', () => {
  const { onNavigate } = show()
  expect(screen.queryByRole('button', { name: '学校管理' })).toBeNull()
  expect(screen.getByRole('button', { name: '内容审核' }).getAttribute('aria-current')).toBe('page')
  fireEvent.click(screen.getByRole('button', { name: '分享记录' }))
  expect(onNavigate).toHaveBeenCalledWith('delivery-attempts')
})
it('provides platform management and logout for an administrator', () => {
  const { onLogout } = show(true)
  expect(screen.getByRole('button', { name: '学校管理' })).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '退出登录' }))
  expect(onLogout).toHaveBeenCalledTimes(1)
})
it('opens mobile navigation and closes it with Escape', () => {
  show()
  const toggle = screen.getByRole('button', { name: '打开导航' })
  fireEvent.click(toggle)
  expect(toggle.getAttribute('aria-expanded')).toBe('true')
  fireEvent.keyDown(window, { key: 'Escape' })
  expect(toggle.getAttribute('aria-expanded')).toBe('false')
  expect(document.activeElement).toBe(toggle)
})

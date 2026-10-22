import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { AppShell } from './index'

class TestResizeObserver { observe() {} unobserve() {} disconnect() {} }
vi.stubGlobal('ResizeObserver', TestResizeObserver)
Object.defineProperty(window, 'matchMedia', { writable: true, value: (query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() }) })
afterEach(cleanup)
function show(platformAdmin = false) {
  const onNavigate = vi.fn()
  const onLogout = vi.fn().mockResolvedValue(undefined)
  render(<MemoryRouter><AppShell active="posts" onNavigate={onNavigate} username="测试运营" platformAdmin={platformAdmin} onLogout={onLogout}><p>学校内容</p></AppShell></MemoryRouter>)
  return { onNavigate, onLogout }
}
it('shows role-appropriate navigation and routes business selections', () => {
  const { onNavigate } = show()
  expect(screen.queryByRole('menuitem', { name: '学校管理' })).toBeNull()
  expect(screen.getByRole('menuitem', { name: '内容审核' }).getAttribute('aria-current')).toBe('page')
  fireEvent.click(screen.getByRole('menuitem', { name: '群发任务' }))
  expect(onNavigate).toHaveBeenCalledWith('broadcast-tasks')
})
it('provides platform management and logout for an administrator', async () => {
  const { onLogout } = show(true)
  expect(screen.getByRole('menuitem', { name: '学校管理' })).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '账号菜单' }))
  fireEvent.click(await screen.findByRole('menuitem', { name: '退出登录' }))
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

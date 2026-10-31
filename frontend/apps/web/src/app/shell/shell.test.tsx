import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { visibleNavigationKeys } from '../navigation'
import { AppShell, navigation } from './index'
import { helpHref, pageHelp } from './help'

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', TestResizeObserver)
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }),
})
afterEach(cleanup)
function show(platformAdmin = false) {
  const onNavigate = vi.fn()
  const onLogout = vi.fn().mockResolvedValue(undefined)
  render(
    <MemoryRouter>
      <AppShell
        active="posts"
        onNavigate={onNavigate}
        username="测试运营"
        platformAdmin={platformAdmin}
        allowedKeys={visibleNavigationKeys(platformAdmin, 'operator')}
        onLogout={onLogout}
      >
        <p>学校内容</p>
      </AppShell>
    </MemoryRouter>,
  )
  return { onNavigate, onLogout }
}
it('shows role-appropriate navigation and routes business selections', () => {
  const { onNavigate } = show()
  expect(screen.queryByRole('menuitem', { name: '学校管理' })).toBeNull()
  expect(screen.getByRole('menuitem', { name: '内容审核' }).getAttribute('aria-current')).toBe(
    'page',
  )
  fireEvent.click(screen.getByRole('menuitem', { name: '群发任务' }))
  expect(onNavigate).toHaveBeenCalledWith('broadcast-tasks')
  fireEvent.click(screen.getByRole('menuitem', { name: '手机发送任务' }))
  expect(onNavigate).toHaveBeenCalledWith('mobile-delivery')
  expect(screen.getByRole('menuitem', { name: '手机设备' })).toBeTruthy()
  expect(screen.getByRole('menuitem', { name: '发送账号' })).toBeTruthy()
})
it('provides platform management and logout for an administrator', async () => {
  const { onLogout } = show(true)
  expect(screen.getByRole('menuitem', { name: '学校管理' })).toBeTruthy()
  expect(screen.getByRole('menuitem', { name: '手机设备' })).toBeTruthy()
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
it('collapses the desktop sidebar while keeping the active page and account accessible', () => {
  show()
  fireEvent.click(screen.getByRole('button', { name: '收起侧栏' }))
  expect(document.querySelector('.workspace-shell')?.classList.contains('is-collapsed')).toBe(true)
  expect(screen.getByRole('menuitem', { name: '内容审核' }).getAttribute('aria-current')).toBe(
    'page',
  )
  expect(screen.getByRole('button', { name: '账号菜单' })).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '展开侧栏' }))
  expect(document.querySelector('.workspace-shell')?.classList.contains('is-collapsed')).toBe(false)
})

it('opens static help without routing or changing the school selection', () => {
  const { onNavigate } = show()
  expect(screen.getByRole('link', { name: '帮助中心' }).getAttribute('href')).toBe('/docs/')
  const help = screen.getByRole('link', { name: '使用说明' })
  expect(help.getAttribute('href')).toBe('/docs/guide/operator/moderation')
  help.addEventListener('click', (event) => event.preventDefault(), { once: true })
  fireEvent.click(help)
  expect(onNavigate).not.toHaveBeenCalled()
})
it('maps every navigation entry to a fixed manual without business parameters', () => {
  expect(Object.keys(pageHelp).sort()).toEqual(navigation.map((n) => n.key).sort())
  for (const item of navigation) {
    expect(helpHref(item.key)).toMatch(/^\/docs\/guide\/[a-z/-]+(?:#[a-z-]+)?$/)
  }
  expect(helpHref('unknown')).toBe('/docs/guide/')
})

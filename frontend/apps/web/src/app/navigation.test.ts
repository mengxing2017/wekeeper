import { expect, test } from 'vitest'
import { canNavigate, navigation, resolveNavigation, visibleNavigationKeys } from './navigation'

test('功能分类不代替数据范围与角色权限', () => {
  for (const key of ['execution-devices', 'execution-accounts']) {
    const item = navigation.find(n => n.key === key)!
    expect(item.domain).toBe('business')
    expect(item.scope).toBe('selectable')
    expect(canNavigate(item, false, 'operator')).toBe(true)
    expect(canNavigate(item, false, 'member')).toBe(false)
  }
  const overview = navigation.find(n => n.key === 'wecom-assets')!
  expect(overview.domain).toBe('business')
  expect(canNavigate(overview, false, 'admin')).toBe(false)
  expect(canNavigate(overview, true)).toBe(true)
  expect(navigation.find(n => n.key === 'settings')?.scope).toBe('school')
  expect(visibleNavigationKeys(false, 'operator')).not.toContain('settings')
  expect(visibleNavigationKeys(false, 'admin')).toContain('settings')
  expect(visibleNavigationKeys(false, 'member')).toEqual([])
})

test('历史入口兼容，未知入口保留给显式错误页处理', () => {
  expect(resolveNavigation('execution-devices', 'accounts')).toBe('execution-accounts')
  expect(resolveNavigation('delivery-drafts', null)).toBe('broadcast-tasks')
  expect(resolveNavigation('delivery-attempts', null)).toBe('broadcast-tasks')
  expect(resolveNavigation('mobile-delivery', 'tasks')).toBe('mobile-delivery')
  expect(resolveNavigation('unknown', null)).toBe('unknown')
  expect(new Set(navigation.map(n => n.key)).size).toBe(navigation.length)
})

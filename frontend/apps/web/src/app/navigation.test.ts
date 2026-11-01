import { expect, test } from 'vitest'
import { canNavigate, navigation, resolveNavigation, visibleNavigationKeys } from './navigation'

test('功能分类不代替数据范围与角色权限', () => {
  for (const key of ['execution-devices', 'wecom-senders']) {
    const item = navigation.find(n => n.key === key)!
    expect(item.domain).toBe(key === 'wecom-senders' ? 'wecom' : 'mobile')
    expect(item.scope).toBe('selectable')
    expect(canNavigate(item, false, 'operator')).toBe(true)
    expect(canNavigate(item, false, 'member')).toBe(false)
  }
  const overview = navigation.find(n => n.key === 'wecom-assets')!
  expect(overview.domain).toBe('wecom')
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

test('独立手机任务属于学校业务，平台管理员不能代替学校成员权限', () => {
  const item = navigation.find(n => n.key === 'mobile-execution-tasks')!
  expect(item.domain).toBe('mobile')
  expect(item.scope).toBe('school')
  expect(canNavigate(item, false, 'operator')).toBe(true)
  expect(canNavigate(item, false, 'admin')).toBe(true)
  expect(canNavigate(item, true)).toBe(false)
})

test('共享手机目标绑定属于平台资源，学校不保留旧设备授权入口', () => {
  const item = navigation.find(n => n.key === 'shared-mobile-resources')!
  expect(item.domain).toBe('mobile')
  expect(item.scope).toBe('global')
  expect(canNavigate(item, false, 'admin')).toBe(false)
  expect(canNavigate(item, true)).toBe(true)
  expect(navigation.find(n => n.key === 'mobile-device-grants')).toBeUndefined()
})

test('三业务域与平台支撑分别组织；历史账号书签不伪装为官方身份', () => {
  expect(new Set(navigation.map(item => item.domain))).toEqual(new Set(['forum', 'wecom', 'mobile', 'support']))
  expect(navigation.find(item => item.key === 'wecom-directory')?.domain).toBe('wecom')
  expect(navigation.find(item => item.key === 'platform-resources')?.domain).toBe('forum')
  expect(navigation.find(item => item.key === 'execution-accounts')?.hidden).toBe(true)
  expect(resolveNavigation('execution-devices', 'accounts')).toBe('execution-accounts')
  expect(resolveNavigation('execution-accounts', null)).not.toBe('wecom-senders')
})

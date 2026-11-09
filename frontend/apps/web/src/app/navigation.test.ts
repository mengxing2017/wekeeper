import { expect, test } from 'vitest'
import { canNavigate, navigation, visibleNavigationKeys } from './navigation'

test('功能分类不代替数据范围与角色权限', () => {
  for (const key of ['wecom-senders']) {
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
  for (const key of ['settings','members','schools','accounts']) expect(navigation.find(n => n.key === key)).toBeUndefined()
  expect(visibleNavigationKeys(false, 'operator')).not.toContain('settings')
  expect(visibleNavigationKeys(false, 'admin')).not.toContain('settings')
  expect(visibleNavigationKeys(false, 'member')).toEqual([])
})

test('只注册当前业务入口', () => {
  expect(navigation.find(n => n.key === 'mobile-delivery')).toBeUndefined()
  expect(new Set(navigation.map(n => n.key)).size).toBe(navigation.length)
})

test('手机任务与目标群使用独立移动权限，学校角色不能放行', () => {
  for (const key of ['mobile-execution-tasks', 'mobile-targets']) {
    const item = navigation.find(n => n.key === key)!
    expect(item.domain).toBe('mobile')
    expect(item.scope).toBe('mobile')
    expect(canNavigate(item, false, 'admin')).toBe(false)
    expect(canNavigate(item, false, undefined, 'operator')).toBe(true)
    expect(canNavigate(item, false, undefined, 'admin')).toBe(true)
    expect(canNavigate(item, true)).toBe(true)
  }
})

test('共享手机目标绑定属于平台资源，学校不保留旧设备授权入口', () => {
  const item = navigation.find(n => n.key === 'shared-mobile-resources')!
  expect(item.domain).toBe('mobile')
  expect(item.scope).toBe('global')
  expect(canNavigate(item, false, 'admin')).toBe(false)
  expect(canNavigate(item, true)).toBe(true)
  expect(navigation.find(n => n.key === 'mobile-device-grants')).toBeUndefined()
})

test('保留消息接收、两类分发业务与系统支撑，论坛入口已移除', () => {
  expect(new Set(navigation.map(item => item.domain))).toEqual(new Set(['source', 'wecom', 'mobile', 'support']))
  expect(navigation.find(item => item.key === 'wecom-directory')?.domain).toBe('wecom')
  for (const key of ['posts','comments','cases','resources','platform-resources']) expect(navigation.find(item => item.key === key)).toBeUndefined()
  expect(navigation.find(item => item.key === 'execution-accounts')).toBeUndefined()
})

test('消息分发路由是调用方凭据的子项，不注册独立导航', () => {
  const credential = navigation.find(item => item.key === 'source-credentials')!
  expect(credential.label).toBe('调用方凭据')
  expect(credential.domain).toBe('source')
  expect(credential.scope).toBe('global')
  expect(canNavigate(credential, true)).toBe(true)
  expect(canNavigate(credential, false, 'admin', 'admin')).toBe(false)
  expect(canNavigate(credential, false, 'operator', 'operator')).toBe(false)
  expect(navigation.find(item => item.key === 'source-routes')).toBeUndefined()
})


test('消息接收管理提供独立目录与路由执行配置入口，按权限展示', () => {
  const directory = navigation.find(item => item.key === 'source-routing-directory')!
  const configuration = navigation.find(item => item.key === 'source-route-configurations')!
  expect(directory.label).toBe('业务单位与分组')
  expect(configuration.label).toBe('路由执行配置')
  for (const item of [directory, configuration]) {
    expect(item.domain).toBe('source')
    expect(canNavigate(item, true)).toBe(true)
    expect(canNavigate(item, false, 'operator', 'operator')).toBe(false)
  }
  expect(canNavigate(directory, false, 'admin', 'admin')).toBe(false)
  expect(canNavigate(configuration, false, 'admin')).toBe(true)
  expect(canNavigate(configuration, false, undefined, 'admin')).toBe(true)
  expect(visibleNavigationKeys(false, 'operator', 'operator', true)).toContain(configuration.key)
})


test('收件与回执提供直接菜单，沿用路由工作空间管理授权',()=>{
 const receipts=navigation.find(item=>item.key==='source-receipts')!
 expect(receipts.label).toBe('收件与回执')
 expect(receipts.domain).toBe('source')
 expect(canNavigate(receipts,true)).toBe(true)
 expect(canNavigate(receipts,false,undefined,'admin')).toBe(true)
 expect(canNavigate(receipts,false,'operator','operator')).toBe(false)
 expect(visibleNavigationKeys(false,'operator','operator',true)).toContain('source-receipts')
})

test('关键字规则提供独立消息接收管理入口，工作空间管理员可见',()=>{
 const item=navigation.find(item=>item.key==='keyword-rules')!
 expect(item.label).toBe('关键字规则')
 expect(item.domain).toBe('source')
 expect(canNavigate(item,true)).toBe(true)
 expect(canNavigate(item,false,undefined,'admin')).toBe(true)
 expect(canNavigate(item,false,'operator','operator')).toBe(false)
})

test('广告素材有直接入口，单位专项管理不依赖分发工作空间角色',()=>{
 const item=navigation.find(item=>item.key==='advertisements')!
 expect(item.label).toBe('广告素材与共享');expect(item.domain).toBe('source')
 expect(canNavigate(item,false,undefined,undefined,false,true)).toBe(true)
 expect(canNavigate(item,false,undefined,undefined,false,false)).toBe(false)
 expect(canNavigate(item,false,undefined,'admin')).toBe(true)
 expect(visibleNavigationKeys(false,undefined,undefined,false,true)).toContain('advertisements')
})

test('广告序列轮询提供独立入口并要求路由配置权限',()=>{
 const item=navigation.find(item=>item.key==='advertisement-rotations')!
 expect(item.label).toBe('广告序列轮询')
 expect(item.domain).toBe('source')
 expect(canNavigate(item,true)).toBe(true)
 expect(canNavigate(item,false,undefined,'admin')).toBe(true)
 expect(canNavigate(item,false,'operator','operator')).toBe(false)
 expect(canNavigate(item,false,undefined,undefined,false,true)).toBe(false)
})

test('makes automatic tasks discoverable to both workspace operators without granting route configuration',()=>{
 expect(visibleNavigationKeys(false,'operator','operator')).toContain('source-automation-tasks')
 expect(visibleNavigationKeys(false,undefined,'operator')).toContain('source-automation-tasks')
 expect(visibleNavigationKeys(false,'operator','operator')).not.toContain('source-route-configurations')
})

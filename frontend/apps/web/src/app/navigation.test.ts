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


test('消息接收管理保留路由执行配置，移除独立单位分组入口', () => {
  expect(navigation.find(item => item.key === 'source-routing-directory')).toBeUndefined()
  const configuration = navigation.find(item => item.key === 'source-route-configurations')!
  expect(configuration.label).toBe('路由执行配置')
  for (const item of [configuration]) {
    expect(item.domain).toBe('source')
    expect(canNavigate(item, true)).toBe(true)
    expect(canNavigate(item, false, 'operator', 'operator')).toBe(false)
  }
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

test('敏感词拦截仅向系统管理员开放',()=>{
 const item=navigation.find(item=>item.key==='keyword-rules')!
 expect(item.label).toBe('敏感词拦截')
 expect(item.domain).toBe('source')
 expect(canNavigate(item,true)).toBe(true)
 expect(canNavigate(item,false,undefined,'admin')).toBe(false)
 expect(canNavigate(item,false,'operator','operator')).toBe(false)
})

test('广告投放管理使用系统管理授权，不依赖工作空间角色',()=>{
 const item=navigation.find(item=>item.key==='advertisements')!
 expect(item.label).toBe('广告素材与投放');expect(item.domain).toBe('source')
 expect(canNavigate(item,true)).toBe(true)
 expect(canNavigate(item,false,undefined,undefined,false,true)).toBe(false)
 expect(canNavigate(item,false,undefined,'admin')).toBe(false)
 expect(visibleNavigationKeys(false,undefined,undefined,false,true)).not.toContain('advertisements')
})

test('广告轮询入口使用系统管理授权',()=>{
 const item=navigation.find(item=>item.key==='advertisement-rotations')!
 expect(item.label).toBe('广告序列轮询');expect(canNavigate(item,true)).toBe(true)
 expect(canNavigate(item,false,undefined,'admin')).toBe(false)
})

test('makes automatic tasks discoverable to both workspace operators without granting route configuration',()=>{
 expect(visibleNavigationKeys(false,'operator','operator')).toContain('source-automation-tasks')
 expect(visibleNavigationKeys(false,undefined,'operator')).toContain('source-automation-tasks')
 expect(visibleNavigationKeys(false,'operator','operator')).not.toContain('source-route-configurations')
})

test('路由授权可进入执行与回执页面，不依赖工作空间角色',()=>{
 const keys=visibleNavigationKeys(false,undefined,undefined,true)
 expect(keys).toContain('source-route-configurations')
 expect(keys).toContain('source-receipts')
 expect(keys).toContain('source-automation-tasks')
})

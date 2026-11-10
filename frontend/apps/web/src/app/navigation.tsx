import type { ReactNode } from 'react'
import {
  ApartmentOutlined,
  AuditOutlined,
  ClusterOutlined,
  MessageOutlined,
  MobileOutlined,
  SafetyCertificateOutlined,
  SendOutlined,
  SnippetsOutlined,
  TeamOutlined,
  UserOutlined,
} from '@ant-design/icons'

export type NavigationDomain = 'source' | 'wecom' | 'mobile' | 'support'
export const navigationDomains = [{ key: 'source', label: '消息接收管理' }, { key: 'wecom', label: '企业微信分发管理' }, { key: 'mobile', label: '移动端分发管理' }, { key: 'support', label: '系统管理' }] as const
export interface NavigationItem {
  key: string
  label: string
  description: string
  domain: NavigationDomain
  scope: 'wecom' | 'global' | 'selectable' | 'mobile'
  capability: 'automation.read' | 'advertising.manage' | 'routing.configure' | 'mobile.operate' | 'wecom.operate' | 'wecom.read' | 'wecom.admin' | 'global.manage' | 'resources.read'
  icon: ReactNode
  hidden?: boolean
}
export const navigation: NavigationItem[] = [
  {
    key: 'groups',
    icon: <TeamOutlined />,
    label: '客户群',
    description: '维护分发工作空间对应的企业微信客户群。',
    domain: 'wecom',
    scope: 'wecom',
    capability: 'wecom.read',
  },
  {
    key: 'templates',
    icon: <SnippetsOutlined />,
    label: '消息模板',
    description: '管理模板正文、版本与发布状态。',
    domain: 'wecom',
    scope: 'wecom',
    capability: 'wecom.read',
  },
  {
    key: 'broadcast-tasks',
    icon: <SendOutlined />,
    label: '群发任务',
    description: '统一创建客户群任务，查看成员执行和各群结果。',
    domain: 'wecom',
    scope: 'wecom',
    capability: 'wecom.read',
  },
  {
    key: 'execution-devices',
    icon: <MobileOutlined />,
    label: '手机设备',
    description: '维护共享手机和执行程序；设备备注不认证实际登录身份。',
    domain: 'mobile',
    scope: 'global',
    capability: 'global.manage',
  },
  { key: 'shared-mobile-resources', icon: <MobileOutlined />, label: '手机群消息使用管理', description: '两步开通移动分发工作空间的手机群消息使用范围，处理申请和使用问题。', domain: 'mobile', scope: 'global', capability: 'global.manage' },
  { key: 'mobile-execution-tasks', icon: <MessageOutlined />, label: '手机任务', description: '独立管理手机发送任务，预览、确认并查看执行记录。', domain: 'mobile', scope: 'mobile', capability: 'mobile.operate' },
  { key: 'mobile-targets', icon: <MessageOutlined />, label: '手机目标群', description: '管理独立手机搜索目标和设备绑定，与官方客户群隔离。', domain: 'mobile', scope: 'mobile', capability: 'mobile.operate' },
  { key: 'wecom-senders', icon: <UserOutlined />, label: '发送账号', description: '查看已核实的官方成员及分发工作空间发送资格，与手机账号无关。', domain: 'wecom', scope: 'selectable', capability: 'resources.read' },
  {
    key: 'audit',
    icon: <AuditOutlined />,
    label: '操作审计',
    description: '查询系统操作、执行人、对象与请求记录。',
    domain: 'support',
    scope: 'global',
    capability: 'global.manage',
  },
  { key: 'wecom-workspaces', icon: <TeamOutlined />, label: '工作空间与权限', description: '查看可用工作空间；系统管理员可创建工作空间并维护成员授权。', domain: 'wecom', scope: 'global', capability: 'resources.read' },
  {
    key: 'wecom-assets',
    icon: <ClusterOutlined />,
    label: '客户群总览',
    description: '客户群目录、运营成员统计与同步任务。',
    domain: 'wecom',
    scope: 'global',
    capability: 'global.manage',
  },
  {
    key: 'wecom-directory',
    icon: <ApartmentOutlined />,
    label: '企业微信集成',
    description: '同步应用可见成员与部门，查看同步状态。',
    domain: 'wecom',
    scope: 'global',
    capability: 'global.manage',
  },
  {
    key: 'administrators',
    icon: <SafetyCertificateOutlined />,
    label: '系统管理员',
    description: '查询、新增、编辑和删除系统管理员，维护启用状态与登录密码。',
    domain: 'support',
    scope: 'global',
    capability: 'global.manage',
  },
  {key:'source-credentials',label:'调用方凭据',description:'签发下游消息推送凭据，维护消息分发路由。',domain:'source',scope:'global',capability:'global.manage',icon:<SafetyCertificateOutlined/>},
  {key:'source-routing-directory',label:'业务单位与分组',description:'维护消息路由使用的业务单位、受众分组及目标目录。',domain:'source',scope:'global',capability:'global.manage',icon:<ApartmentOutlined/>},
  {key:'advertisements',label:'广告素材与共享',description:'维护单位广告、授权共享和正文版本，接收方主动选用。',domain:'source',scope:'global',capability:'advertising.manage',icon:<SnippetsOutlined/>},
  {key:'advertisement-rotations',label:'广告序列轮询',description:'选择授权路由，配置自动广告序列、轮询进度和投放模式。',domain:'source',scope:'global',capability:'routing.configure',icon:<SnippetsOutlined/>},
  {key:'keyword-rules',label:'敏感词拦截',description:'维护来源消息的敏感词拦截规则。',domain:'source',scope:'global',capability:'global.manage',icon:<SafetyCertificateOutlined/>},
  {key:'source-automation-tasks',label:'自动链路任务',description:'查看自动任务、通道执行结果与执行证据，停止或推进可恢复目标。',domain:'source',scope:'global',capability:'automation.read',icon:<SendOutlined/>},
  {key:'source-receipts',label:'收件与回执',description:'按获授权路由查看消息收件、版本处理、检查记录及实际发送结果。',domain:'source',scope:'global',capability:'routing.configure',icon:<MessageOutlined/>},
  {key:'source-route-configurations',label:'路由执行配置',description:'配置获授权路由的发送通道、工作空间、目标群及自动运行状态。',domain:'source',scope:'global',capability:'routing.configure',icon:<SendOutlined/>},
]

export function canNavigate(item: NavigationItem, platformAdmin: boolean, schoolRole?: string, mobileRole?: string, routingAdmin = false, advertisingAdmin = false): boolean {
  switch (item.capability) {
    case 'automation.read': return platformAdmin || ['admin','operator'].includes(schoolRole??'') || ['admin','operator'].includes(mobileRole??'')
    case 'advertising.manage': return platformAdmin || advertisingAdmin || routingAdmin || schoolRole === 'admin' || mobileRole === 'admin'
    case 'routing.configure': return platformAdmin || routingAdmin || schoolRole === 'admin' || mobileRole === 'admin'
    case 'mobile.operate': return platformAdmin || mobileRole === 'admin' || mobileRole === 'operator'
    case 'wecom.operate': return schoolRole === 'admin' || schoolRole === 'operator'
    case 'global.manage': return platformAdmin
    case 'resources.read': return platformAdmin || schoolRole === 'admin' || schoolRole === 'operator'
    case 'wecom.admin': return platformAdmin || schoolRole === 'admin'
    case 'wecom.read': return platformAdmin || schoolRole === 'admin' || schoolRole === 'operator'
  }
}
export function visibleNavigationKeys(platformAdmin: boolean, schoolRole?: string, mobileRole?: string, routingAdmin = false, advertisingAdmin = false) {
  return navigation.filter(item => canNavigate(item, platformAdmin, schoolRole, mobileRole, routingAdmin, advertisingAdmin)).map(item => item.key)
}

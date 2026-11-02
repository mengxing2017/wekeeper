import type { ReactNode } from 'react'
import {
  AlertOutlined,
  ApartmentOutlined,
  AppstoreOutlined,
  AuditOutlined,
  BankOutlined,
  ClusterOutlined,
  CommentOutlined,
  FileSearchOutlined,
  FilterOutlined,
  MessageOutlined,
  MobileOutlined,
  SafetyCertificateOutlined,
  SendOutlined,
  SettingOutlined,
  SnippetsOutlined,
  TeamOutlined,
  UserOutlined,
  UsergroupAddOutlined,
} from '@ant-design/icons'

export type NavigationDomain = 'forum' | 'wecom' | 'mobile' | 'support'
export const navigationDomains = [{ key: 'forum', label: '论坛管理' }, { key: 'wecom', label: '企业微信分发管理' }, { key: 'mobile', label: '移动端分发管理' }, { key: 'support', label: '平台与系统管理' }] as const
export interface NavigationItem {
  key: string
  label: string
  description: string
  domain: NavigationDomain
  scope: 'school' | 'global' | 'selectable'
  capability: 'school.operate' | 'school.read' | 'school.admin' | 'global.manage' | 'resources.read'
  icon: ReactNode
  hidden?: boolean
}
export const navigation: NavigationItem[] = [
  {
    key: 'posts',
    icon: <FileSearchOutlined />,
    label: '内容审核',
    description: '查看校园内容，完成审核与展示管理。',
    domain: 'forum',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'comments',
    icon: <CommentOutlined />,
    label: '评论审核',
    description: '审核评论与回复，维护社区交流秩序。',
    domain: 'forum',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'cases',
    icon: <AlertOutlined />,
    label: '举报与反馈',
    description: '处理用户举报与反馈，保留处置记录。',
    domain: 'forum',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'resources',
    icon: <FilterOutlined />,
    label: '展示与屏蔽词',
    description: '管理校园展示资源、服务入口与内容规则。',
    domain: 'forum',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'groups',
    icon: <TeamOutlined />,
    label: '客户群',
    description: '维护学校对应的企业微信客户群。',
    domain: 'wecom',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'templates',
    icon: <SnippetsOutlined />,
    label: '消息模板',
    description: '管理模板正文、版本与发布状态。',
    domain: 'wecom',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'broadcast-tasks',
    icon: <SendOutlined />,
    label: '群发任务',
    description: '统一创建客户群任务，查看成员执行和各群结果。',
    domain: 'wecom',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'execution-devices',
    icon: <MobileOutlined />,
    label: '手机设备',
    description: '维护共享手机和执行程序；设备备注不认证实际登录身份。',
    domain: 'mobile',
    scope: 'selectable',
    capability: 'resources.read',
  },
  { key: 'shared-mobile-resources', icon: <MobileOutlined />, label: '手机群消息使用管理', description: '两步开通学校的手机群消息使用范围，处理申请和使用问题。', domain: 'mobile', scope: 'global', capability: 'global.manage' },
  { key: 'mobile-execution-tasks', icon: <MessageOutlined />, label: '手机任务', description: '选择学校目标群，预览共享手机分配、人工确认和查看任务记录。', domain: 'mobile', scope: 'school', capability: 'school.operate' },
  { key: 'mobile-targets', icon: <MessageOutlined />, label: '手机目标群', description: '管理独立手机搜索目标和设备绑定，与官方客户群隔离。', domain: 'mobile', scope: 'school', capability: 'school.operate' },
  { key: 'wecom-senders', icon: <UserOutlined />, label: '发送账号', description: '查看已核实的官方成员及学校发送资格，与手机账号无关。', domain: 'wecom', scope: 'selectable', capability: 'resources.read' },
  {
    key: 'members',
    icon: <UsergroupAddOutlined />,
    label: '学校成员与权限',
    description: '查看本校成员及角色，学校授权由平台管理员维护。',
    domain: 'support',
    scope: 'school',
    capability: 'school.admin',
  },
  {
    key: 'audit',
    icon: <AuditOutlined />,
    label: '操作审计',
    description: '追踪学校内的关键操作与变更记录。',
    domain: 'support',
    scope: 'school',
    capability: 'school.read',
  },
  {
    key: 'settings',
    icon: <SettingOutlined />,
    label: '学校设置',
    description: '设置学校信息、审核方式与主题颜色。',
    domain: 'support',
    scope: 'school',
    capability: 'school.admin',
  },
  {
    key: 'schools',
    icon: <BankOutlined />,
    label: '学校管理',
    description: '查看学校列表，创建学校并进入学校配置或成员管理。',
    domain: 'support',
    scope: 'global',
    capability: 'global.manage',
  },
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
    key: 'accounts',
    icon: <UserOutlined />,
    label: '登录账号管理',
    description: '管理全平台普通账号、登录状态与企业微信身份映射。',
    domain: 'support',
    scope: 'global',
    capability: 'global.manage',
  },
  {
    key: 'administrators',
    icon: <SafetyCertificateOutlined />,
    label: '平台管理员',
    description: '查看平台管理员身份与状态，管理登录密码。',
    domain: 'support',
    scope: 'global',
    capability: 'global.manage',
  },
  {
    key: 'platform-resources',
    icon: <AppstoreOutlined />,
    label: '平台默认展示配置',
    description: '维护跨学校共用的默认资源与屏蔽词。',
    domain: 'forum',
    scope: 'global',
    capability: 'global.manage',
  },
]

export function canNavigate(item: NavigationItem, platformAdmin: boolean, schoolRole?: string): boolean {
  switch (item.capability) {
    case 'school.operate': return schoolRole === 'admin' || schoolRole === 'operator'
    case 'global.manage': return platformAdmin
    case 'resources.read': return platformAdmin || schoolRole === 'admin' || schoolRole === 'operator'
    case 'school.admin': return platformAdmin || schoolRole === 'admin'
    case 'school.read': return platformAdmin || schoolRole === 'admin' || schoolRole === 'operator'
  }
}
export function visibleNavigationKeys(platformAdmin: boolean, schoolRole?: string) {
  return navigation.filter(item => canNavigate(item, platformAdmin, schoolRole)).map(item => item.key)
}

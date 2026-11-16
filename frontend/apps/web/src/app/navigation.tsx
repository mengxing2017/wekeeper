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
  icon: ReactNode
  hidden?: boolean
}
export const navigation: NavigationItem[] = [
  {
    key: 'groups',
    icon: <TeamOutlined />,
    label: '客户群',
    description: '维护企业微信客户群。',
    domain: 'wecom',


  },
  {
    key: 'broadcast-tasks',
    icon: <SendOutlined />,
    label: '群发任务',
    description: '统一创建客户群任务，查看成员执行和各群结果。',
    domain: 'wecom',


  },
  {
    key: 'execution-devices',
    icon: <MobileOutlined />,
    label: '手机设备',
    description: '维护共享手机和执行程序；设备备注不认证实际登录身份。',
    domain: 'mobile',


  },
  { key: 'shared-mobile-resources', icon: <MobileOutlined />, label: '手机群消息使用管理', description: '开通手机群消息使用范围，处理申请和使用问题。', domain: 'mobile' },
  { key: 'mobile-execution-tasks', icon: <MessageOutlined />, label: '手机任务', description: '独立管理手机发送任务，预览、确认并查看执行记录。', domain: 'mobile' },
  { key: 'mobile-targets', icon: <MessageOutlined />, label: '手机目标群', description: '管理独立手机搜索目标和设备绑定，与官方客户群隔离。', domain: 'mobile' },
  { key: 'mobile-receiving-configurations', icon: <MessageOutlined />, label: '手机接收配置', description: '按业务单位维护接收群与候选手机。', domain: 'mobile' },
  { key: 'wecom-receiving-configurations', icon: <MessageOutlined />, label: '企业微信接收配置', description: '按业务单位维护接收群与发送账号。', domain: 'wecom' },
  { key: 'wecom-senders', icon: <UserOutlined />, label: '发送账号', description: '查看已核实的官方成员及发送资格，与手机账号无关。', domain: 'wecom' },
  {
    key: 'audit',
    icon: <AuditOutlined />,
    label: '操作审计',
    description: '查询系统操作、执行人、对象与请求记录。',
    domain: 'support',


  },
  {
    key: 'wecom-assets',
    icon: <ClusterOutlined />,
    label: '客户群总览',
    description: '客户群目录、运营成员统计与同步任务。',
    domain: 'wecom',


  },
  {
    key: 'wecom-directory',
    icon: <ApartmentOutlined />,
    label: '企业微信集成',
    description: '同步应用可见成员与部门，查看同步状态。',
    domain: 'wecom',


  },
  {
    key: 'administrators',
    icon: <SafetyCertificateOutlined />,
    label: '系统管理员',
    description: '查询、新增、编辑和删除系统管理员，维护启用状态与登录密码。',
    domain: 'support',


  },
  {key:'source-credentials',label:'调用方凭据',description:'签发下游消息推送凭据，维护消息分发路由。',domain:'source',icon:<SafetyCertificateOutlined/>},
  {key:'advertisements',label:'广告素材与投放',description:'维护广告正文及业务单位、平台、全局投放绑定。',domain:'source',icon:<SnippetsOutlined/>},
  {key:'advertisement-rotations',label:'广告序列轮询',description:'绑定全局、平台或业务单位的广告；按业务单位分别记录广告轮询进度。',domain:'source',icon:<SnippetsOutlined/>},
  {key:'keyword-rules',label:'敏感词拦截',description:'维护来源消息的敏感词拦截规则。',domain:'source',icon:<SafetyCertificateOutlined/>},
  {key:'source-automation-tasks',label:'即时消息与发送结果',description:'查看即时消息、通道委托及实际发送结果。',domain:'source',icon:<SendOutlined/>},
  {key:'source-receipts',label:'收件与回执',description:'按路由查看消息收件与实际发送结果。',domain:'source',icon:<MessageOutlined/>},
  {key:'source-route-configurations',label:'路由执行配置',description:'按业务单位维护通道接收配置。',domain:'source',icon:<SendOutlined/>},
]

export function canNavigate(_item: NavigationItem, platformAdmin: boolean): boolean { return platformAdmin }
export function visibleNavigationKeys(platformAdmin: boolean) {
  return navigation.filter(item => canNavigate(item, platformAdmin)).map(item => item.key)
}

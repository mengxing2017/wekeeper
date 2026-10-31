import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Avatar, Breadcrumb, Button, Dropdown, Menu } from 'antd'
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
  MenuFoldOutlined,
  MenuUnfoldOutlined,
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
import { Link } from 'react-router-dom'
import { helpHref } from './help'

export interface NavigationItem {
  key: string
  label: string
  description: string
  group: string
  icon: ReactNode
}
export const navigation: NavigationItem[] = [
  {
    key: 'posts',
    icon: <FileSearchOutlined />,
    label: '内容审核',
    description: '查看校园内容，完成审核与展示管理。',
    group: '内容运营',
  },
  {
    key: 'comments',
    icon: <CommentOutlined />,
    label: '评论审核',
    description: '审核评论与回复，维护社区交流秩序。',
    group: '内容运营',
  },
  {
    key: 'cases',
    icon: <AlertOutlined />,
    label: '举报与反馈',
    description: '处理用户举报与反馈，保留处置记录。',
    group: '内容运营',
  },
  {
    key: 'resources',
    icon: <FilterOutlined />,
    label: '展示与屏蔽词',
    description: '管理校园展示资源、服务入口与内容规则。',
    group: '内容运营',
  },
  {
    key: 'groups',
    icon: <TeamOutlined />,
    label: '客户群',
    description: '维护学校对应的企业微信客户群。',
    group: '企业微信',
  },
  {
    key: 'templates',
    icon: <SnippetsOutlined />,
    label: '消息模板',
    description: '管理模板正文、版本与发布状态。',
    group: '企业微信',
  },
  {
    key: 'broadcast-tasks',
    icon: <SendOutlined />,
    label: '群发任务',
    description: '统一创建客户群任务，查看成员执行和各群结果。',
    group: '企业微信',
  },
  {
    key: 'mobile-delivery',
    icon: <MessageOutlined />,
    label: '手机发送任务',
    description: '配置手机执行、目标群并人工确认发送任务。',
    group: '企业微信',
  },
  {
    key: 'execution-devices',
    icon: <MobileOutlined />,
    label: '手机设备与发送账号',
    description: '登记手机、配对执行程序并维护真实发送账号。',
    group: '平台管理',
  },
  {
    key: 'members',
    icon: <UsergroupAddOutlined />,
    label: '学校成员',
    description: '查看本校成员及角色，学校授权由平台管理员维护。',
    group: '系统管理',
  },
  {
    key: 'audit',
    icon: <AuditOutlined />,
    label: '操作审计',
    description: '追踪学校内的关键操作与变更记录。',
    group: '系统管理',
  },
  {
    key: 'settings',
    icon: <SettingOutlined />,
    label: '学校设置',
    description: '设置学校信息、审核方式与主题颜色。',
    group: '系统管理',
  },
  {
    key: 'schools',
    icon: <BankOutlined />,
    label: '学校管理',
    description: '查看学校列表，创建学校并进入学校配置或成员管理。',
    group: '平台管理',
  },
  {
    key: 'wecom-assets',
    icon: <ClusterOutlined />,
    label: '客户群管理',
    description: '客户群目录、运营成员统计与同步任务。',
    group: '平台管理',
  },
  {
    key: 'wecom-directory',
    icon: <ApartmentOutlined />,
    label: '企业通讯录',
    description: '同步应用可见成员与部门，查看同步状态。',
    group: '平台管理',
  },
  {
    key: 'accounts',
    icon: <UserOutlined />,
    label: '账号管理',
    description: '管理全平台普通账号、登录状态与企业微信身份映射。',
    group: '平台管理',
  },
  {
    key: 'administrators',
    icon: <SafetyCertificateOutlined />,
    label: '本地管理员管理',
    description: '查看平台管理员身份与状态，管理登录密码。',
    group: '平台管理',
  },
  {
    key: 'platform-resources',
    icon: <AppstoreOutlined />,
    label: '平台展示配置',
    description: '维护跨学校共用的默认资源与屏蔽词。',
    group: '平台管理',
  },
]

export const schoolRoleLabel = (role?: string) =>
  ({ member: '普通成员', operator: '运营', admin: '学校管理员', revoked: '已撤销学校授权' })[
    role ?? ''
  ] ?? '无学校授权'

export function AppShell({
  children,
  active,
  onNavigate,
  username,
  platformAdmin,
  allowedKeys,
  schoolRole,
  onLogout,
  pageExtra,
}: {
  children: ReactNode
  active: string
  onNavigate: (key: string) => void
  username: string
  platformAdmin: boolean
  allowedKeys?: string[]
  schoolRole?: string
  onLogout: () => Promise<void>
  pageExtra?: ReactNode
}) {
  const [collapsed, setCollapsed] = useState(false)
  const [open, setOpen] = useState(false)
  const [signingOut, setSigningOut] = useState(false)
  const toggle = useRef<HTMLButtonElement>(null)
  const sidebar = useRef<HTMLElement>(null)
  const current = navigation.find((item) => item.key === active) ?? {
    group: '学校工作区',
    label: '学校工作区',
    description: '选择学校后使用已授权的管理功能。',
  }
  useEffect(() => {
    if (!open) return
    sidebar.current?.querySelector<HTMLElement>('[role=menuitem]')?.focus()
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false)
        toggle.current?.focus()
      }
    }
    window.addEventListener('keydown', close)
    return () => window.removeEventListener('keydown', close)
  }, [open])
  return (
    <div className={`workspace-shell ${collapsed ? 'is-collapsed' : ''}`}>
      <a className="skip-link" href="#workspace-main">
        跳到主要内容
      </a>
      <header className="global-header">
        <Link className="header-brand" to="/admin">
          <span className="brand-mark">W</span>
          <strong>WeKeeper</strong>
        </Link>
        <span className="header-workspace">运营工作台</span>
        <div className="header-actions">
          <a className="help-center-link" href="/docs/">
            帮助中心
          </a>
          <Link className="community-link" to="/community">
            校园社区 ↗
          </Link>
          <Dropdown
            trigger={['click']}
            menu={{
              items: [
                {
                  key: 'role',
                  label: platformAdmin ? '平台管理员' : schoolRoleLabel(schoolRole),
                  disabled: true,
                },
                { type: 'divider' },
                {
                  key: 'logout',
                  label: signingOut ? '退出中' : '退出登录',
                  danger: true,
                  disabled: signingOut,
                },
              ],
              onClick: async ({ key }) => {
                if (key === 'logout') {
                  setSigningOut(true)
                  try {
                    await onLogout()
                  } finally {
                    setSigningOut(false)
                  }
                }
              },
            }}
          >
            <Button type="text" className="account-trigger" aria-label="账号菜单">
              <Avatar size={28}>{username.trim().charAt(0).toUpperCase() || '?'}</Avatar>
              <span className="header-username">{username}</span>
            </Button>
          </Dropdown>
        </div>
      </header>
      <div className="mobile-bar">
        <Button
          type="text"
          ref={(instance) => {
            toggle.current = instance as HTMLButtonElement | null
          }}
          className="icon-button"
          aria-label="打开导航"
          aria-expanded={open}
          aria-controls="app-sidebar"
          onClick={() => setOpen(!open)}
          icon={<MenuUnfoldOutlined />}
        />
        <span>{current.group}</span>
      </div>
      {open && (
        <Button
          className="nav-scrim"
          aria-label="关闭导航"
          onClick={() => {
            setOpen(false)
            toggle.current?.focus()
          }}
        />
      )}
      <aside ref={sidebar} id="app-sidebar" className={`app-sidebar ${open ? 'open' : ''}`}>
        <nav className="app-navigation" aria-label="运营导航">
          <Menu
            mode="inline"
            inlineCollapsed={collapsed && !open}
            selectedKeys={[active]}
            items={[
              '内容运营',
              '企业微信',
              '系统管理',
              ...(platformAdmin ? ['平台管理'] : []),
            ].flatMap((group) => {
              const items = navigation.filter(
                (item) => item.group === group && (!allowedKeys || allowedKeys.includes(item.key)),
              )
              return items.length
                ? [
                    {
                      type: 'group' as const,
                      key: group,
                      label: collapsed && !open ? null : group,
                      children: items.map((item) => ({
                        key: item.key,
                        label: item.label,
                        title: item.label,
                        icon: <span aria-hidden="true">{item.icon}</span>,
                        'aria-current': active === item.key ? ('page' as const) : undefined,
                      })),
                    },
                  ]
                : []
            })}
            onClick={({ key }) => {
              onNavigate(key)
              setOpen(false)
            }}
          />
        </nav>
        <div className="sidebar-footer">
          <Button
            type="text"
            block
            aria-label={collapsed ? '展开侧栏' : '收起侧栏'}
            onClick={() => setCollapsed(!collapsed)}
            icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
          >
            {!collapsed && '收起导航'}
          </Button>
        </div>
      </aside>
      <div className="workspace-content">
        <main id="workspace-main" className="page-content">
          <div className="page-header">
            <Breadcrumb
              className="breadcrumb"
              items={[{ title: '运营工作台' }, { title: current.group }, { title: current.label }]}
            />
            <header className="page-heading">
              <div>
                <h1>{current.label}</h1>
                <a href={helpHref(active)}>使用说明</a>
              </div>
              {pageExtra}
            </header>
          </div>
          <div className="page-body">{children}</div>
        </main>
      </div>
    </div>
  )
}

import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Avatar, Breadcrumb, Button, Dropdown, Menu } from 'antd'
import { MenuFoldOutlined, MenuUnfoldOutlined } from '@ant-design/icons'
import { navigation, navigationDomains } from '../navigation'
export { navigation } from '../navigation'
import { Link } from 'react-router-dom'
import { helpHref } from './help'

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
  allowedKeys: string[]
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
    domain: 'business',
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
        <span>{navigationDomains.find(d => d.key === current.domain)?.label}</span>
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
            items={navigationDomains.flatMap(({ key: domain, label: domainLabel }) => {
              const items = navigation.filter(
                (item) => item.domain === domain && allowedKeys.includes(item.key),
              )
              return items.length
                ? [
                    {
                      type: 'group' as const,
                      key: domain,
                      label: collapsed && !open ? null : domainLabel,
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
              items={[{ title: '运营工作台' }, { title: navigationDomains.find(d => d.key === current.domain)?.label }, { title: current.label }]}
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

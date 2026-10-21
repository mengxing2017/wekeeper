import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'

export interface NavigationItem { key: string; label: string; description: string; group: string }
export const navigation: NavigationItem[] = [
  { key: 'posts', label: '内容审核', description: '查看校园内容，完成审核与展示管理。', group: '内容运营' },
  { key: 'comments', label: '评论审核', description: '审核评论与回复，维护社区交流秩序。', group: '内容运营' },
  { key: 'cases', label: '举报与反馈', description: '处理用户举报与反馈，保留处置记录。', group: '内容运营' },
  { key: 'resources', label: '展示与屏蔽词', description: '管理校园展示资源、服务入口与内容规则。', group: '内容运营' },
  { key: 'groups', label: '客户群', description: '维护学校对应的企业微信客户群。', group: '企业微信' },
  { key: 'templates', label: '消息模板', description: '管理模板正文、版本与发布状态。', group: '企业微信' },
  { key: 'delivery-drafts', label: '分享草稿', description: '组合已审核内容，预览待分享正文。', group: '企业微信' },
  { key: 'delivery-attempts', label: '分享记录', description: '查看客户端结果，人工核对不确定的分享。', group: '企业微信' },
  { key: 'members', label: '学校成员', description: '查看本校成员及角色，学校授权由平台管理员维护。', group: '系统管理' },
  { key: 'audit', label: '操作审计', description: '追踪学校内的关键操作与变更记录。', group: '系统管理' },
  { key: 'settings', label: '学校设置', description: '设置学校信息、审核方式与主题颜色。', group: '系统管理' },
  { key: 'schools', label: '学校管理', description: '查看学校列表，创建学校并进入学校配置或成员管理。', group: '平台管理' },
  { key: 'wecom-assets', label: '客户群管理', description: '客户群目录、运营成员统计与同步任务。', group: '平台管理' },
  { key: 'wecom-directory', label: '企业通讯录', description: '同步应用可见成员与部门，查看同步状态。', group: '平台管理' },
  { key: 'accounts', label: '账号管理', description: '管理全平台普通账号、登录状态与企业微信身份映射。', group: '平台管理' },
  { key: 'administrators', label: '本地管理员管理', description: '查看平台管理员身份与状态，管理登录密码。', group: '平台管理' },
  { key: 'platform-resources', label: '平台展示配置', description: '维护跨学校共用的默认资源与屏蔽词。', group: '平台管理' },
]

export const schoolRoleLabel = (role?: string) => ({ member: '普通成员', operator: '运营', admin: '学校管理员', revoked: '已撤销学校授权' })[role ?? ''] ?? '无学校授权';

export function AppShell({ children, active, onNavigate, username, platformAdmin, allowedKeys, schoolRole, onLogout }: {
  children: ReactNode; active: string; onNavigate: (key: string) => void;
  username: string; platformAdmin: boolean; allowedKeys?: string[]; schoolRole?: string; onLogout: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false)
  const [signingOut, setSigningOut] = useState(false)
  const toggle = useRef<HTMLButtonElement>(null)
  const sidebar = useRef<HTMLElement>(null)
  const current = navigation.find(item => item.key === active) ?? { group: '学校工作区', label: '学校工作区', description: '选择学校后使用已授权的管理功能。' }
  useEffect(() => {
    if (!open) return
    sidebar.current?.querySelector<HTMLButtonElement>('button')?.focus()
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setOpen(false); toggle.current?.focus() }
    }
    window.addEventListener('keydown', close)
    return () => window.removeEventListener('keydown', close)
  }, [open])
  return <div className="workspace-shell">
    <a className="skip-link" href="#workspace-main">跳到主要内容</a>
    <header className="mobile-bar"><button ref={toggle} className="icon-button" aria-label="打开导航" aria-expanded={open} aria-controls="app-sidebar" onClick={() => setOpen(!open)}>☰</button><strong>WeKeeper</strong><span>运营工作台</span></header>
    {open && <button className="nav-scrim" aria-label="关闭导航" onClick={() => { setOpen(false); toggle.current?.focus() }} />}
    <aside ref={sidebar} id="app-sidebar" className={`app-sidebar ${open ? 'open' : ''}`}>
      <Link className="sidebar-brand" to="/admin"><span className="brand-mark">W</span><span><strong>WeKeeper</strong><small>校园内容运营</small></span></Link>
      <nav className="app-navigation" aria-label="运营导航">
        {['内容运营', '企业微信', '系统管理', ...(platformAdmin ? ['平台管理'] : [])].filter(group => navigation.some(item => item.group === group && (!allowedKeys || allowedKeys.includes(item.key)))).map(group => <div className="nav-group" key={group}><p>{group}</p>{navigation.filter(item => item.group === group && (!allowedKeys || allowedKeys.includes(item.key))).map(item => <button key={item.key} aria-current={active === item.key ? 'page' : undefined} onClick={() => { onNavigate(item.key); setOpen(false) }}><span className="nav-indicator" aria-hidden="true" />{item.label}</button>)}</div>)}
      </nav>
      <div className="sidebar-links"><Link to="/community">校园社区 ↗</Link><Link to="/wecom">人工分享 ↗</Link></div>
      <div className="account-menu"><span className="account-avatar">{username.slice(0, 1).toUpperCase()}</span><div><strong>{username}</strong><small>{platformAdmin ? '平台管理员' : schoolRoleLabel(schoolRole)}</small></div><button disabled={signingOut} onClick={async () => { setSigningOut(true); try { await onLogout() } finally { setSigningOut(false) } }} aria-label="退出登录">{signingOut ? '退出中' : '退出'}</button></div>
    </aside>
    <div className="workspace-content"><div className="breadcrumb"><span>运营工作台</span><span aria-hidden="true">/</span><span>{current.group}</span><span aria-hidden="true">/</span><strong>{current.label}</strong></div><main id="workspace-main" className="page-content"><header className="page-heading"><div><h1>{current.label}</h1><p>{current.description}</p></div><span className="workspace-badge">{current.group === '平台管理' ? '全平台管理' : '学校工作空间'}</span></header>{children}</main><footer className="workspace-footer">WeKeeper · 校园社区与人工分享</footer></div>
  </div>
}

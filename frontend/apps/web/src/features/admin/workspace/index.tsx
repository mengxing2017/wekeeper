import {MobileDispatchRecords} from '../../operations/mobile-execution/dispatch-records'
import {SourceAutomationTasks} from '../../operations/source-access/automation'
import {ReceivingConfigurations} from '../../operations/receiving-configurations'
import {Advertising} from '../../operations/advertising'
import { KeywordRules } from '../../operations/source-access/keywords'
import { AssignedRoutes } from '../../operations/source-access/assigned'
import { SourceAccess } from '../../operations/source-access'
import { OfficialSenders } from '../../operations/wecom-senders'
import { MobileExecutionTasks } from '../../operations/mobile-execution/tasks'
import { SharedMobileResources } from '../mobile-resources'
import { MobileTargets } from '../../operations/mobile-execution/targets'
import { navigation, visibleNavigationKeys, type NavigationPage } from '../../../app/navigation'
import { ListPanel } from '../../../shared/list-panel'
import { formatDate } from '../../../shared/format-date'
import { Disclosure } from '../../../shared/disclosure'
import { Navigate, useLocation, useSearchParams } from 'react-router-dom'
import { canonicalNavigation } from '../../../app/navigation-location'
import { AppShell } from '../../../app/shell'
import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Alert,
  Button,
  Descriptions,
  Drawer,
  Input,
  Space,
  Table,
  Tag,
} from 'antd'
import { ExecutionDevices } from '../../admin/devices'
import BroadcastTasks from '../../operations/broadcast-tasks'
import { Directory } from './directory'
import { Assets, GroupPicker } from './assets'
import { Editor } from './editor'
import { Administrators } from './administrators'
import { AuditEvents } from '../audit'
import { workspace, type Row, type Json, type Resource, type User } from './api'
export { label } from '../../../shared/label'
import { label } from '../../../shared/label'
import { SessionGate } from '../../identity'
export { SessionGate } from '../../identity'
const states: Record<string, string> = {
  member: '普通成员',
  operator: '运营',
  admin: '分发管理员',
  revoked: '已撤销分发授权',
  prepared: '等待客户端结果',
  client_reported_success: '客户端报告成功',
  client_reported_failure: '客户端报告失败',
  unknown: '结果不确定',
  cancelled: '已取消',
  operator_confirmed: '人工确认已发送',
  operator_rejected: '人工确认未发送',
}
export const stateLabel = (v: Json | undefined) => states[label(v)] ?? label(v)
export function Workspace() {
  const [params] = useSearchParams()
  const location = useLocation()
  const canonical = canonicalNavigation(params)
  if (canonical) return <Navigate replace to={{pathname: location.pathname, search: `?${canonical}`, hash: location.hash}} />
  return (
    <SessionGate showSessionBar={false}>
      {(user, logout) => <Admin user={user} logout={logout} />}
    </SessionGate>
  )
}
function Admin({ user, logout }: { user: User; logout: () => Promise<void> }) {
  const cache = useQueryClient()
  const [params, setParams] = useSearchParams()
  const allowedKeys = visibleNavigationKeys(user.platform_admin)
  const tab = params.get('view') ?? allowedKeys[0] ?? ''
  const knownPage = navigation.find(n => n.key === tab)
  const navigate = (view: string) => setParams(previous => { const next = new URLSearchParams(previous); next.set('view',view); next.delete('subview'); return next })
  const refresh = async () => { await cache.invalidateQueries({queryKey:['business']}) }
  function renderPage(page: NavigationPage) {
    switch (page.kind) {
      case 'mobile-dispatch-records': return <MobileDispatchRecords />
      case 'source-credentials': return <SourceAccess />
      case 'advertisements': return <Advertising />
      case 'keyword-rules': return <KeywordRules />
      case 'source-automation-tasks': return <SourceAutomationTasks />
      case 'source-routes': return <AssignedRoutes key={page.mode} receiptsOnly={page.mode === 'receipts'} />
      case 'audit': return <AuditEvents />
      case 'receiving-configurations': return <ReceivingConfigurations key={page.channel} channel={page.channel} admin={user.platform_admin} platformAdmin={user.platform_admin}/>
      case 'mobile-execution-tasks': return <MobileExecutionTasks admin onNavigate={navigate} />
      case 'mobile-targets': return <MobileTargets admin />
      case 'wecom-senders': return <OfficialSenders />
      case 'broadcast-tasks': return <BroadcastTasks admin platformAdmin />
      case 'groups': return <Panel administrator platformAdmin resource="groups" refresh={refresh} />
      case 'execution-devices': return <ExecutionDevices />
      case 'mobile-business-schemes': return <SharedMobileResources />
      case 'wecom-assets': return <Assets />
      case 'wecom-directory': return <Directory />
      case 'administrators': return <Administrators user={user} />
      default: {
        const unsupported: never = page
        throw new Error(`Unknown page: ${JSON.stringify(unsupported)}`)
      }
    }
  }
  const page = !user.platform_admin ? <Alert type="warning" title="当前账号没有此功能的访问权限" />
    : !knownPage ? <Alert type="warning" title="页面不存在" /> : renderPage(knownPage.page)
  return <AppShell active={tab} onNavigate={navigate} username={user.display_name} platformAdmin={user.platform_admin} allowedKeys={allowedKeys} onLogout={logout}>{page}</AppShell>
}
function Panel({
  resource,
  refresh,
  administrator,
  platformAdmin,
}: {
  platformAdmin: boolean
  administrator: boolean
  resource: Resource
  refresh: () => Promise<void>
}) {
  const [page, setPage] = useState(1),
    [search, setSearch] = useState('')
  const [editing, setEditing] = useState<Row>()
  const [selectedId, setSelectedId] = useState<string>()
  const [creating, setCreating] = useState(false)
  const query = `?${new URLSearchParams({ 'page[number]': String(page), 'page[size]': '30', ...(search ? { search } : {}) })}`
  const q = useQuery({
    queryKey: ['business', resource, query],
    queryFn: ({ signal }) => workspace.list(resource, query, signal),
  })
  const mutation = async (path: string, body: unknown, method = 'POST') => {
    await workspace.mutate(path, body, method)
    await refresh()
  }
  return (
    <>
      {q.error && <Alert type="error" title={q.error.message} />}
      {resource === 'groups' && administrator && (
        <>
          {(creating || editing) && (
            <Drawer
              open
              size="large"
              title={editing ? '编辑客户群' : '关联客户群'}
              onClose={() => {
                setCreating(false)
                setEditing(undefined)
              }}
            >
              {!editing && <GroupPicker onSaved={refresh} />}
              {(editing || platformAdmin) && (
                <Disclosure
                  title={<>{editing ? '编辑客户群' : '平台例外：手填已知群标识'}</>}
                  open={!!editing}
                >
                  <Editor
                    key={editing?.id ?? 'new'}
                    fields={[
                      { name: 'name', title: '群名称', value: label(editing?.name) },
                      {
                        name: 'corp_id',
                        title: '企业 ID（与应用配置一致）',
                        readOnly: !!editing,
                        value: label(editing?.corp_id),
                      },
                      {
                        name: 'chat_id',
                        title: '客户群 ID（保存后不可更换）',
                        readOnly: !!editing,
                        value: label(editing?.chat_id),
                      },
                      {
                        name: 'active',
                        title: '启用状态',
                        value: editing ? String(editing.active) : 'true',
                        options: [
                          { value: 'true', label: '启用' },
                          { value: 'false', label: '撤销' },
                        ],
                      },
                    ]}
                    onSave={(v) =>
                      mutation('groups', {
                        ...v,
                        id: editing?.id,
                        revision: editing?.revision ?? 0,
                        active: v.active === 'true',
                      })
                    }
                  />
                </Disclosure>
              )}
            </Drawer>
          )}
        </>
      )}
      <ListPanel
        filters={
          <>
            <Space wrap>
              <Input.Search
                aria-label="搜索内容"
                placeholder="搜索标题或正文"
                onSearch={(v) => {
                  setSearch(v)
                  setPage(1)
                }}
              />
              <Button onClick={() => q.refetch()} loading={q.isFetching}>
                刷新
              </Button>
            </Space>
          </>
        }
        title="查询结果"
        actions={
          resource === 'groups' && administrator && (
            <Button
              type="primary"
              onClick={() => {
                setEditing(undefined)
                setCreating(true)
              }}
            >
              关联客户群
            </Button>
          )
        }
      >
        <Table<Row>
          rowKey="id"
          loading={q.isFetching}
          dataSource={q.data?.items ?? []}
          pagination={false}
          scroll={{ x: 880 }}
          columns={[
            {
              title: '标题 / 名称',
              key: 'title',
              width: 240,
              render: (_, r) => (
                <Button className="record-title" type="link" onClick={() => setSelectedId(r.id)}>
                  {label(r.title ?? r.name ?? r.group_name ?? r.display_name ?? r.action) || r.id}
                </Button>
              ),
            },
            {
              title: '内容摘要',
              key: 'summary',
              ellipsis: true,
              render: (_, r) =>
                label(r.content ?? r.body ?? r.detail ?? r.contact ?? r.chat_id) || '—',
            },
            {
              title: '状态',
              key: 'status',
              width: 140,
              render: (_, r) => (
                <Tag
                  color={
                    r.active === true
                      ? 'success'
                      : r.active === false
                        ? 'error'
                        : 'processing'
                  }
                >
                  {stateLabel(
                    r.state ?? r.role ??
                      (r.active === true ? '启用' : r.active === false ? '停用' : r.kind),
                  )}
                </Tag>
              ),
            },
            {
              title: '创建时间',
              key: 'created',
              width: 180,
              render: (_, r) => formatDate(r.created_at),
            },
            {
              title: '操作',
              key: 'actions',
              width: 150,
              fixed: 'right',
              render: (_, r) => (
                <Space size={8} wrap>
                  <Button type="link" onClick={() => setSelectedId(r.id)}>
                    详情
                  </Button>
                  {resource === 'groups' && administrator && (
                    <Button type="link" onClick={() => setEditing(r)}>
                      编辑
                    </Button>
                  )}
                </Space>
              ),
            },
          ]}
        />
      </ListPanel>
      {q.data?.items
        .filter((r) => r.id === selectedId)
        .map((r) => (
          <Drawer
            key={r.id}
            open
            size="large"
            title={label(r.title ?? r.name ?? r.action) || '记录详情'}
            onClose={() => setSelectedId(undefined)}
          >
            <Descriptions
              title="基本信息"
              column={1}
              bordered
              items={[
                { key: 'id', label: '记录 ID', children: r.id },
                {
                  key: 'status',
                  label: '状态',
                  children: <Tag>{stateLabel(r.state ?? r.kind)}</Tag>,
                },
                { key: 'created', label: '创建时间', children: formatDate(r.created_at) },
              ]}
            />
            <section className="detail-section">
              {r.content && <p className="text-content">{label(r.content)}</p>}
              {r.body && <p className="text-content">{label(r.body)}</p>}
              {r.reconciliation && (
                <p>
                  核对结果：{stateLabel(r.reconciliation)} · {label(r.reconciliation_note)}
                </p>
              )}
              <small>
                {formatDate(r.created_at)} {r.id}
              </small>
              {r.detail && <pre>{label(r.detail)}</pre>}
              {r.url && (
                <p>
                  <a href={label(r.url)} target="_blank" rel="noreferrer">
                    查看资源链接
                  </a>
                </p>
              )}
              {resource === 'groups' && administrator && (
                <Button
                  type="primary"
                  onClick={() => {
                    setEditing(r)
                    setSelectedId(undefined)
                  }}
                >
                  编辑
                </Button>
              )}
            </section>
          </Drawer>
        ))}
    </>
  )
}

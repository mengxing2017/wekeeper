import {SourceAutomationTasks} from '../../operations/source-access/automation'
import {Advertising} from '../../operations/advertising'
import {advertising} from '../../operations/advertising/api'
import { KeywordRules } from '../../operations/source-access/keywords'
import { AssignedRoutes } from '../../operations/source-access/assigned'
import { RoutingDirectory } from '../../operations/source-access/directory'
import { SourceAccess } from '../../operations/source-access'
import { TemplateManager } from './template-manager'
import { OfficialSenders } from '../../operations/wecom-senders'
import { MobileExecutionTasks } from '../../operations/mobile-execution/tasks'
import { SharedMobileResources } from '../mobile-resources'
import { MobileTargets } from '../../operations/mobile-execution/targets'
import { navigation, visibleNavigationKeys } from '../../../app/navigation'
import { MobileWorkspacePanel } from '../../operations/mobile-execution/workspaces'
import { mobileWorkspaces } from '../../operations/mobile-execution/api'
import { ListPanel } from '../../../shared/list-panel'
import { formatDate } from '../../../shared/format-date'
import { Disclosure } from '../../../shared/disclosure'
import { useSearchParams } from 'react-router-dom'
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
  Empty,
} from 'antd'
import { ExecutionDevices } from '../../admin/devices'
import BroadcastTasks from '../../operations/broadcast-tasks'
import { Directory } from './directory'
import { Assets, GroupPicker } from './assets'
import { Editor } from './editor'
import { Administrators } from './administrators'
import { AuditEvents } from '../audit'
import { WecomWorkspacePanel } from '../../operations/wecom-senders/workspaces'
import { wecomWorkspaces, type WecomWorkspace } from '../../operations/wecom-senders/api'
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
  return (
    <SessionGate showSessionBar={false}>
      {(user, logout) => <Admin user={user} logout={logout} />}
    </SessionGate>
  )
}
function Admin({ user, logout }: { user: User; logout: () => Promise<void> }) {
  const cache = useQueryClient()
  const [params, setParams] = useSearchParams()
  const official = useQuery({ queryKey: ['wecom-workspaces'], queryFn: wecomWorkspaces.list, enabled: params.get('view') !== 'audit' })
  const mobile = useQuery({ queryKey: ['mobile-workspaces'], queryFn: mobileWorkspaces.list, enabled: params.get('view') !== 'audit' })
  const wecom = official.data?.items.find(w => w.id === params.get('wecom_workspace')) ?? official.data?.items[0]
  const phone = mobile.data?.items.find(w => w.id === params.get('workspace')) ?? mobile.data?.items[0]
  const routingAdmin = [...(official.data?.items ?? []), ...(mobile.data?.items ?? [])].some(w => w.role === 'admin')
  const advertisingGrants=useQuery({queryKey:['advertisement-navigation'],queryFn:advertising.directory,refetchInterval:10000,enabled:!user.platform_admin&&params.get('view')!=='audit'})
  const advertisingAdmin=advertisingGrants.data?.units.some(u=>u.can_edit||u.can_share)??false
  const allowedKeys = visibleNavigationKeys(user.platform_admin, wecom?.role, phone?.role, routingAdmin, advertisingAdmin)
  const tab = params.get('view') ?? allowedKeys[0] ?? ''
  const knownPage = navigation.find(n => n.key === tab)
  const navigate = (view: string) => setParams(previous => { const next = new URLSearchParams(previous); next.set('view',view); next.delete('subview'); next.delete('school'); return next })
  const selectWorkspace = (kind: 'workspace' | 'wecom_workspace', id: string) => setParams(previous => { const next = new URLSearchParams(previous); next.set(kind,id); next.delete('school'); return next })
  const refresh = async () => { await cache.invalidateQueries({queryKey:['business']}) }
  const setSubview = (view: string) => setParams(previous => { const next = new URLSearchParams(previous); next.set('subview',view); return next })
  let page
  if ((knownPage?.scope !== 'global' || knownPage?.capability === 'resources.read' || knownPage?.capability === 'routing.configure' || knownPage?.capability === 'advertising.manage') && (official.isPending || mobile.isPending || (tab==='advertisements'&&advertisingGrants.isPending&&!user.platform_admin)) && (!knownPage || !allowedKeys.includes(tab))) page = <Empty description="正在加载分发权限…" />
  else if (!tab) page = <Alert type="info" title="当前账号暂无分发授权，请联系系统管理员。" />
  else if (!knownPage) page = <Alert type="warning" title="页面不存在" />
  else if (knownPage.scope === 'mobile' && mobile.error) page = <Alert type="error" title={mobile.error.message} />
  else if ((knownPage.scope === 'wecom' || knownPage.scope === 'selectable') && official.error) page = <Alert type="error" title={official.error.message} />
  else if (!allowedKeys.includes(tab)) page = <Alert type="warning" title="当前账号没有此功能的访问权限" />
  else if (tab === 'wecom-workspaces') page = <WecomWorkspacePanel manage platformAdmin={user.platform_admin} workspaces={official.data?.items ?? []} selected={wecom} loading={official.isPending} error={official.error} onSelect={id=>selectWorkspace('wecom_workspace',id)}>
    <p>管理当前企业微信分发工作空间及成员授权。客户群、消息模板和群发任务在对应业务页面操作。</p>
  </WecomWorkspacePanel>
  else if (tab === 'source-credentials') page = <SourceAccess platformAdmin={user.platform_admin} />
  else if (tab === 'source-routing-directory') page = <RoutingDirectory />
  else if (tab === 'advertisements') page = <Advertising />
  else if (tab === 'advertisement-rotations') page = <AssignedRoutes key="advertisement-rotations" advertisingOnly platformAdmin={user.platform_admin} />
  else if (tab === 'keyword-rules') page = <KeywordRules />
  else if (tab === 'source-automation-tasks') page = <SourceAutomationTasks />
  else if (tab === 'source-receipts') page = <AssignedRoutes key="source-receipts" receiptsOnly platformAdmin={user.platform_admin} />
  else if (tab === 'source-route-configurations') page = <AssignedRoutes platformAdmin={user.platform_admin} />
  else if (tab === 'audit') page = <AuditEvents />
  else if (knownPage.scope === 'mobile') page = <MobileWorkspacePanel platformAdmin={user.platform_admin} workspaces={mobile.data?.items ?? []} selected={phone} loading={mobile.isPending} error={mobile.error} onSelect={id=>selectWorkspace('workspace',id)}>
    {phone && (tab === 'mobile-execution-tasks' ? <MobileExecutionTasks key={phone.id} school={phone.id} admin={phone.role==='admin'} onNavigate={navigate} /> : <MobileTargets key={phone.id} school={phone.id} admin={phone.role==='admin'} />)}
  </MobileWorkspacePanel>
  else if (knownPage.scope === 'wecom' || knownPage.scope === 'selectable') page = <WecomWorkspacePanel platformAdmin={user.platform_admin} workspaces={official.data?.items ?? []} selected={wecom} loading={official.isPending} error={official.error} onSelect={id=>selectWorkspace('wecom_workspace',id)}>
    {wecom && (tab === 'wecom-senders' ? <OfficialSenders key={wecom.id} school={wecom.id} global={user.platform_admin} /> : tab === 'broadcast-tasks' ? <BroadcastTasks key={wecom.id} school={wecom.id} admin={wecom.role==='admin'} platformAdmin={user.platform_admin} /> : <Panel key={`${wecom.id}:${tab}`} school={wecom} schoolAdmin={wecom.role==='admin'} platformAdmin={user.platform_admin} resource={tab as Resource} refresh={refresh} />)}
  </WecomWorkspacePanel>
  else if (tab === 'execution-devices') page = <ExecutionDevices view={params.get('subview')==='usage'?'usage':'devices'} onViewChange={setSubview} />
  else if (tab === 'shared-mobile-resources') page = <SharedMobileResources />
  else if (tab === 'wecom-assets') page = <Assets />
  else if (tab === 'wecom-directory') page = <Directory />
  else if (tab === 'administrators') page = <Administrators user={user} />
  return <AppShell active={tab} onNavigate={navigate} username={user.display_name} platformAdmin={user.platform_admin} allowedKeys={allowedKeys} schoolRole={wecom?.role} mobileRole={phone?.role} onLogout={logout}>{page}</AppShell>
}
function Panel({
  school,
  resource,
  refresh,
  schoolAdmin,
  platformAdmin,
}: {
  platformAdmin: boolean
  schoolAdmin: boolean
  school: WecomWorkspace
  resource: Resource
  refresh: () => Promise<void>
}) {
  const s = school.id
  const [page, setPage] = useState(1),
    [search, setSearch] = useState('')
  const [editing, setEditing] = useState<Row>()
  const [selectedId, setSelectedId] = useState<string>()
  const [creating, setCreating] = useState(false)
  const query = `?${new URLSearchParams({ 'page[number]': String(page), 'page[size]': '30', ...(search ? { search } : {}) })}`
  const q = useQuery({
    queryKey: ['business', s, resource, query],
    queryFn: ({ signal }) => workspace.list(s, resource, query, signal),
  })
  const mutation = async (path: string, body: unknown, method = 'POST') => {
    await workspace.mutate(s, path, body, method)
    await refresh()
  }
  if(resource==='templates')return <TemplateManager school={s}/>
  return (
    <>
      {q.error && <Alert type="error" title={q.error.message} />}
      {resource === 'groups' && schoolAdmin && (
        <>
          {(creating || editing) && (
            <Drawer
              open
              size="large"
              title={editing ? '编辑分发工作空间群' : '关联客户群'}
              onClose={() => {
                setCreating(false)
                setEditing(undefined)
              }}
            >
              {!editing && <GroupPicker key={school.id} school={school.id} onSaved={refresh} />}
              {(editing || platformAdmin) && (
                <Disclosure
                  title={<>{editing ? '编辑分发工作空间群' : '平台例外：手填已知群标识'}</>}
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
          resource === 'groups' && schoolAdmin && (
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
                <Space size={0}>
                  <Button type="link" onClick={() => setSelectedId(r.id)}>
                    详情
                  </Button>
                  {resource === 'groups' && schoolAdmin && (
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
              {resource === 'groups' && schoolAdmin && (
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

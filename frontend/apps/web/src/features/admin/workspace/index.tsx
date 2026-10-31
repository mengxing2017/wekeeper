import { navigation, visibleNavigationKeys, resolveNavigation } from '../../../app/navigation'
import { SchoolExecutionResources } from '../../operations/mobile-delivery/school-resources'
import { ListPanel } from '../../../shared/list-panel'
import { formatDate } from '../../../shared/format-date'
import { PaginationControls } from '../../../shared/pagination-controls'
import { useDialogs } from '../../../shared/use-dialogs'
import { Disclosure } from '../../../shared/disclosure'
import { useSearchParams } from 'react-router-dom'
import { AppShell, schoolRoleLabel } from '../../../app/shell'
import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Drawer,
  Input,
  Select,
  Space,
  Table,
  Tag,
  Empty,
} from 'antd'
import MobileDelivery, { ExecutionDevices } from '../../operations/mobile-delivery'
import BroadcastTasks from '../../operations/broadcast-tasks'
import { Directory } from './directory'
import { Assets, GroupPicker } from './assets'
import { Members } from './members'
import { Editor } from './editor'
import { Accounts, Schools } from './platform'
import { workspace, type Row, type Json, type Resource, type School, type User } from './api'
export { label } from '../../../shared/label'
import { label } from '../../../shared/label'
import { SessionGate } from '../../identity'
import { useSchools } from '../../school-context'
export { SessionGate } from '../../identity'
const states: Record<string, string> = {
  member: '普通成员',
  operator: '运营',
  admin: '学校管理员',
  revoked: '已撤销学校授权',
  pending: '待审核',
  approved: '已通过',
  rejected: '已拒绝',
  visible: '展示中',
  hidden: '已隐藏',
  deleted: '已删除',
  prepared: '等待客户端结果',
  client_reported_success: '客户端报告成功',
  client_reported_failure: '客户端报告失败',
  unknown: '结果不确定',
  cancelled: '已取消',
  operator_confirmed: '人工确认已发送',
  operator_rejected: '人工确认未发送',
  open: '待处理',
  resolved: '已处理',
  dismissed: '已驳回',
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
  const q = useSchools()
  const [params, setParams] = useSearchParams()
  const selected = params.get('school') ?? ''
  const visibleSchools = q.data?.filter((s) => user.platform_admin || s.active)
  const school = visibleSchools?.find((s) => s.id === (selected || visibleSchools[0]?.id))
  const role = school?.active ? user.grants.find((g) => g.school_id === school.id)?.role : undefined
  const allowedKeys = visibleNavigationKeys(user.platform_admin, role)
  const requestedTab = resolveNavigation(params.get('view') ?? allowedKeys[0] ?? '', params.get('subview'))
  const knownPage = navigation.find(n => n.key === requestedTab)
  const tab = requestedTab
  const denied = !allowedKeys.includes(tab)
  const resourcePage = ['execution-devices', 'execution-accounts'].includes(tab)
  const globalScope = knownPage?.scope === 'global' || (knownPage?.scope === 'selectable' && user.platform_admin && params.get('scope') !== 'school')
  const setSubview = (view: string) => setParams(previous => { const next = new URLSearchParams(previous); next.set('subview', view); return next })
  const navigate = (key: string) =>
    setParams((previous) => {
      const next = new URLSearchParams(previous)
      next.set('view', key)
      next.delete('subview')
      return next
    })
  const enterSchool = (id: string, view: string) => setParams({ school: id, view })
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['schools'] })
    await cache.invalidateQueries({ queryKey: ['business'] })
  }
  return (
    <AppShell
      active={tab}
      onNavigate={navigate}
      username={user.display_name}
      platformAdmin={user.platform_admin}
      allowedKeys={allowedKeys}
      schoolRole={role}
      pageExtra={
        <div className="scope-toolbar">
          {resourcePage && user.platform_admin && <Select aria-label="资源数据范围" value={globalScope ? 'global' : 'school'} options={[{ value: 'school', label: '当前学校使用资源' }, { value: 'global', label: '全平台资源维护' }]} onChange={scope => setParams(previous => { const next = new URLSearchParams(previous); next.set('scope', scope); next.delete('subview'); return next })} />}
          {globalScope ? <Tag>范围：全平台</Tag> : <>
            <div>
              <span className="field-caption">当前学校</span>
              <Select
                aria-label="当前学校"
                className="school-select"
                loading={q.isPending}
                value={school?.id}
                options={visibleSchools?.map((s) => ({ value: s.id, label: s.name }))}
                placeholder="选择学校"
                onChange={async (s) => {
                  await Promise.all([cache.cancelQueries({ queryKey: ['business'] }), cache.cancelQueries({ queryKey: ['mobile'] })])
                  cache.removeQueries({ queryKey: ['business'] })
                  cache.removeQueries({ queryKey: ['mobile'] })
                  setParams((previous) => {
                    const next = new URLSearchParams(previous)
                    next.set('school', s)
                    return next
                  })
                }}
              />
            </div>
          </>}
        </div>
      }
      onLogout={logout}
    >
      {q.error && <Alert type="error" title={q.error.message} showIcon />}
      {!tab ? (q.isPending ? <Empty className="empty-state" description="正在加载学校…" /> : <Alert type="info" title="当前账号没有该学校的后台管理权限，请联系平台管理员。" />)
      : !knownPage ? <Alert type="warning" title="页面不存在或已迁移" description={<Button onClick={() => navigate(allowedKeys[0] ?? 'posts')}>返回可用功能</Button>} />
      : denied ? <Alert type="warning" title="当前账号没有此功能的访问权限" description={<>请核对当前学校与角色，或从导航选择已授权功能。<Button onClick={() => navigate(allowedKeys[0] ?? 'posts')}>返回可用功能</Button></>} />
      : resourcePage && globalScope && user.platform_admin ? (
        <ExecutionDevices key={tab} view={tab === 'execution-accounts' ? 'accounts' : params.get('subview') === 'rules' ? 'rules' : 'devices'} onViewChange={setSubview} />
      ) : tab === 'wecom-assets' && user.platform_admin ? (
        <Assets />
      ) : tab === 'wecom-directory' && user.platform_admin ? (
        <Directory />
      ) : tab === 'schools' && user.platform_admin ? (
        <Schools
          schools={q.data ?? []}
          loading={q.isPending}
          refresh={refresh}
          enter={enterSchool}
        />
      ) : (tab === 'accounts' || tab === 'administrators') && user.platform_admin ? (
        <Accounts
          key={tab}
          administrators={tab === 'administrators'}
          user={user}
          schools={q.data ?? []}
        />
      ) : tab === 'platform-resources' && user.platform_admin ? (
        <GlobalResources />
      ) : q.isPending ? (
        <Empty className="empty-state" description={<>正在加载学校…</>} />
      ) : !school ? (
        <Empty
          className="empty-state"
          description={
            <>
              <h2>尚未选择学校</h2>
              <p>
                {user.platform_admin
                  ? '请在「学校管理」中创建学校，或选择已有学校。'
                  : '请联系管理员为账号分配学校权限。'}
              </p>
              {user.platform_admin && (
                <Button type="primary" onClick={() => navigate('schools')}>
                  创建学校
                </Button>
              )}
            </>
          }
        />
      ) : (
        <div className="business-panel" key={`${school.id}-${tab}`}>
          {resourcePage ? (role === 'admin' || role === 'operator' ? <SchoolExecutionResources key={`${school.id}-${tab}`} school={school.id} accounts={tab === 'execution-accounts'} admin={role === 'admin'} /> : <Alert type="warning" title="查看学校执行资源需要本校运营或管理员授权" />) : tab === 'mobile-delivery' ? (
            <MobileDelivery key={school.id} school={school.id} role={role} subview={params.get('subview') ?? undefined} onSubviewChange={setSubview} />
          ) : tab === 'broadcast-tasks' ? (
            <BroadcastTasks
              key={school.id}
              school={school.id}
              admin={role === 'admin'}
              platformAdmin={user.platform_admin}
            />
          ) : tab === 'members' ? (
            <Members
              key={school.id}
              school={school}
              platformAdmin={user.platform_admin}
              refresh={refresh}
            />
          ) : tab === 'settings' ? (
            <Card title={`${school.name} · 学校设置`}>
              <Settings key={school.id} school={school} refresh={refresh} />
            </Card>
          ) : (
            <Panel
              platformAdmin={user.platform_admin}
              key={`${school.id}:${tab}`}
              schoolAdmin={user.platform_admin || role === 'admin'}
              school={school}
              resource={tab as Resource}
              refresh={refresh}
            />
          )}
        </div>
      )}
    </AppShell>
  )
}
function Settings({ school, refresh }: { school: School; refresh: () => Promise<void> }) {
  return (
    <Editor
      fields={[
        { name: 'name', title: '学校名称', value: school.name },
        {
          name: 'post_audit_mode',
          title: '帖子审核模式',
          value: label(school.config.post_audit_mode) || 'manual',
          options: [
            { value: 'manual', label: '人工审核' },
            { value: 'auto', label: '自动通过' },
          ],
        },
        {
          name: 'comment_audit_mode',
          title: '评论审核模式',
          value: label(school.config.comment_audit_mode) || 'manual',
          options: [
            { value: 'manual', label: '人工审核' },
            { value: 'auto', label: '自动通过' },
          ],
        },
        {
          name: 'theme_color',
          title: '主题颜色',
          type: 'color',
          value: label(school.config.theme_color) || '#25624b',
        },
      ]}
      onSave={async (v) => {
        await workspace.updateSchool(school.id, {
          name: v.name,
          revision: school.revision,
          config: {
            ...school.config,
            post_audit_mode: v.post_audit_mode,
            comment_audit_mode: v.comment_audit_mode,
            theme_color: v.theme_color,
          },
        })
        await refresh()
      }}
    />
  )
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
  school: School
  resource: Resource
  refresh: () => Promise<void>
}) {
  const s = school.id
  const [page, setPage] = useState(1),
    [search, setSearch] = useState(''),
    [status, setStatus] = useState('')
  const [error, setError] = useState(''),
    [busy, setBusy] = useState(false),
    [editing, setEditing] = useState<Row>()
  const [selectedId, setSelectedId] = useState<string>()
  const [creating, setCreating] = useState(false)
  const query = `?${new URLSearchParams({ 'page[number]': String(page), 'page[size]': '30', ...(search ? { search } : {}), ...(status ? { status } : {}) })}`
  const q = useQuery({
    queryKey: ['business', s, resource, query],
    queryFn: ({ signal }) => workspace.list(s, resource, query, signal),
  })
  const dialogs = useDialogs()
  async function act(path: string, body: unknown, method = 'POST') {
    setBusy(true)
    setError('')
    try {
      await workspace.mutate(s, path, body, method)
      await refresh()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const mutation = async (path: string, body: unknown, method = 'POST') => {
    await workspace.mutate(s, path, body, method)
    await refresh()
  }
  return (
    <>
      {dialogs.contextHolder}

      {(error || q.error) && <Alert type="error" title={error || q.error?.message} />}
      {resource === 'resources' && schoolAdmin && (
        <>
          {(creating || editing) && (
            <Drawer
              open
              size="large"
              title={editing ? '编辑展示资源' : '新建展示资源'}
              onClose={() => {
                setCreating(false)
                setEditing(undefined)
              }}
            >
              <ResourceEditor
                key={editing?.id ?? 'new'}
                item={editing}
                save={async (v) => {
                  await mutation('resources', v)
                  setCreating(false)
                  setEditing(undefined)
                }}
              />
            </Drawer>
          )}
        </>
      )}
      {resource === 'groups' && schoolAdmin && (
        <>
          {(creating || editing) && (
            <Drawer
              open
              size="large"
              title={editing ? '编辑学校群' : '关联客户群'}
              onClose={() => {
                setCreating(false)
                setEditing(undefined)
              }}
            >
              {!editing && <GroupPicker key={school.id} school={school.id} onSaved={refresh} />}
              {(editing || platformAdmin) && (
                <Disclosure
                  title={<>{editing ? '编辑学校群' : '平台例外：手填已知群标识'}</>}
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
      {resource === 'templates' && (
        <>
          {creating && (
            <Drawer open size="large" title="新建消息模板" onClose={() => setCreating(false)}>
              <Editor
                fields={[{ name: 'name', title: '模板名称' }]}
                onSave={async (v) => {
                  await mutation('templates', v)
                  setCreating(false)
                }}
              />
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
              {['posts', 'comments'].includes(resource) && (
                <Select
                  aria-label="审核状态"
                  value={status}
                  style={{ width: 140 }}
                  onChange={setStatus}
                  options={[
                    { value: '', label: '全部状态' },
                    ...['pending', 'approved', 'rejected'].map((value) => ({
                      value,
                      label: states[value],
                    })),
                  ]}
                />
              )}
              <Button onClick={() => q.refetch()} loading={q.isFetching}>
                刷新
              </Button>
            </Space>
          </>
        }
        footer={
          [
            'posts',
            'comments',
            'audit',
            'delivery-drafts',
            'delivery-attempts',
            'cases',
            'notifications',
          ].includes(resource) && (
            <PaginationControls
              page={page}
              more={q.data?.page?.has_more}
              busy={q.isFetching}
              change={setPage}
            />
          )
        }
        title="查询结果"
        actions={
          (((resource === 'resources' || resource === 'groups') && schoolAdmin) ||
            resource === 'templates') && (
            <Button
              type="primary"
              onClick={() => {
                setEditing(undefined)
                setCreating(true)
              }}
            >
              {
                { resources: '新建展示资源', groups: '关联客户群', templates: '新建消息模板' }[
                  resource as string
                ]
              }
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
                    r.audit_status === 'approved' || r.active === true
                      ? 'success'
                      : r.audit_status === 'rejected' || r.active === false
                        ? 'error'
                        : 'processing'
                  }
                >
                  {stateLabel(
                    r.audit_status ??
                      r.state ??
                      r.role ??
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
                  {['resources', 'groups'].includes(resource) && schoolAdmin && (
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
                  children: <Tag>{stateLabel(r.audit_status ?? r.state ?? r.kind)}</Tag>,
                },
                { key: 'created', label: '创建时间', children: formatDate(r.created_at) },
              ]}
            />
            <section className="detail-section">
              {error && <Alert type="error" showIcon title={error} />}
              {resource === 'members' && (
                <p>
                  {school.name} · {label(r.username)} · {schoolRoleLabel(label(r.role))}
                </p>
              )}
              {r.content && <p className="text-content">{label(r.content)}</p>}
              {r.body && <p className="text-content">{label(r.body)}</p>}
              {r.contact && <p>联系方式：{label(r.contact)}</p>}
              {r.visibility && <Tag>{stateLabel(r.visibility)}</Tag>}
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
              {['posts', 'comments'].includes(resource) && (
                <Space wrap>
                  {['approved', 'rejected', 'pending'].map((decision) => (
                    <Button
                      type="link"
                      danger={decision === 'rejected'}
                      key={decision}
                      disabled={busy}
                      onClick={async () => {
                        const reason = await dialogs.prompt('审核说明')
                        if (reason !== null)
                          void act(`${resource}/${r.id}/moderation-decisions`, {
                            revision: r.revision,
                            decision,
                            reason,
                          })
                      }}
                    >
                      {states[decision]}
                    </Button>
                  ))}
                  {resource === 'posts' && (
                    <>
                      {['visible', 'hidden', 'deleted'].map((visibility) => (
                        <Button
                          type="link"
                          danger={visibility === 'deleted'}
                          key={visibility}
                          disabled={busy}
                          onClick={() =>
                            act(
                              `posts/${r.id}/visibility`,
                              { revision: r.revision, visibility, top_sort: r.top_sort ?? 0 },
                              'PUT',
                            )
                          }
                        >
                          {states[visibility]}
                        </Button>
                      ))}
                      <Button
                        type="link"
                        disabled={busy}
                        onClick={() =>
                          act(
                            `posts/${r.id}/visibility`,
                            {
                              revision: r.revision,
                              visibility: r.visibility,
                              top_sort: r.top_sort ? 0 : 100,
                            },
                            'PUT',
                          )
                        }
                      >
                        {r.top_sort ? '取消置顶' : '置顶'}
                      </Button>
                    </>
                  )}
                </Space>
              )}
              {resource === 'cases' && r.state === 'open' && (
                <Editor
                  fields={[
                    { name: 'resolution', title: '处理说明' },
                    {
                      name: 'state',
                      title: '处理结果',
                      options: [
                        { value: 'resolved', label: '已处理' },
                        { value: 'dismissed', label: '驳回' },
                      ],
                    },
                  ]}
                  onSave={(v) => mutation(`cases/${r.id}`, { ...v, revision: r.revision }, 'PUT')}
                />
              )}
              {['resources', 'groups'].includes(resource) && schoolAdmin && (
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
              {resource === 'templates' && (
                <>
                  <Editor
                    fields={[
                      {
                        name: 'body',
                        title:
                          '新版本正文（支持 {{school.name}}、{{post.title}}、{{post.summary}}、{{post.detail_url}}）',
                        type: 'textarea',
                      },
                    ]}
                    onSave={(v) => mutation(`templates/${r.id}/versions`, v)}
                  />
                  {Array.isArray(r.versions) &&
                    r.versions.map((v) => {
                      const version = v as Row
                      return (
                        <div key={version.id}>
                          <pre>{label(version.body)}</pre>
                          {version.published ? (
                            <Tag>已发布 · {version.id}</Tag>
                          ) : (
                            <Button
                              disabled={busy}
                              onClick={() =>
                                act(`template-versions/${version.id}/publish`, {
                                  revision: version.revision,
                                })
                              }
                            >
                              发布此版本
                            </Button>
                          )}
                        </div>
                      )
                    })}
                </>
              )}
              {resource === 'delivery-drafts' && (
                <a href={`/wecom?school=${s}&draft=${r.id}`}>预览草稿与客户群入口说明 ↗</a>
              )}
              {resource === 'delivery-attempts' && schoolAdmin && (
                <>
                  <p>客户端结果与人工核对独立保存；不确定结果不会自动重发。</p>
                  <Editor
                    fields={[
                      {
                        name: 'decision',
                        title: '核对结论',
                        options: [
                          { value: 'operator_confirmed', label: '人工确认已发送' },
                          { value: 'operator_rejected', label: '人工确认未发送' },
                        ],
                      },
                      { name: 'reason', title: '核对依据', type: 'textarea' },
                    ]}
                    onSave={(v) => mutation(`delivery-attempts/${r.id}/reconciliations`, v)}
                  />
                </>
              )}
            </section>
          </Drawer>
        ))}
    </>
  )
}
function ResourceEditor({ item, save }: { item?: Row; save: (v: unknown) => Promise<unknown> }) {
  return (
    <Editor
      fields={[
        {
          name: 'kind',
          title: '类型',
          value: label(item?.kind),
          options: [
            ['banner', '轮播'],
            ['qrcode', '二维码'],
            ['service', '客服'],
            ['legal', '法律文本'],
            ['tag', '标签'],
            ['blocked_word', '屏蔽词'],
          ].map(([value, label]) => ({ value, label })),
        },
        { name: 'title', title: '标题或屏蔽词', value: label(item?.title) },
        {
          name: 'content',
          title: '正文 / 说明',
          type: 'textarea',
          optional: true,
          value: label(item?.content),
        },
        { name: 'url', title: '图片或跳转链接', optional: true, value: label(item?.url) },
        { name: 'sort_order', title: '排序', type: 'number', value: label(item?.sort_order) || 0 },
        {
          name: 'starts_at',
          title: '生效时间',
          type: 'datetime',
          optional: true,
          value: label(item?.starts_at),
        },
        {
          name: 'ends_at',
          title: '失效时间',
          type: 'datetime',
          optional: true,
          value: label(item?.ends_at),
        },
        {
          name: 'active',
          title: '状态',
          value: item ? String(item.active) : 'true',
          options: [
            { value: 'true', label: '启用' },
            { value: 'false', label: '停用' },
          ],
        },
      ]}
      onSave={(v) =>
        save({
          ...v,
          id: item?.id,
          revision: item?.revision ?? 0,
          active: v.active === 'true',
          sort_order: Number(v.sort_order),
          starts_at: v.starts_at || null,
          ends_at: v.ends_at || null,
        })
      }
    />
  )
}
function GlobalResources() {
  const q = useQuery({ queryKey: ['global-resources'], queryFn: workspace.globalResources })
  const [item, setItem] = useState<Row>()
  const [open, setOpen] = useState(false)
  return (
    <section>
      <Alert
        type="info"
        showIcon
        title="学校有同类启用配置时优先使用学校配置；全局屏蔽词始终生效。"
      />
      <Card
        className="data-panel"
        title="平台资源列表"
        extra={
          <Button
            type="primary"
            onClick={() => {
              setItem(undefined)
              setOpen(true)
            }}
          >
            新建平台资源
          </Button>
        }
      >
        <Table<Row>
          rowKey="id"
          loading={q.isFetching}
          dataSource={q.data ?? []}
          pagination={{ pageSize: 10 }}
          scroll={{ x: 700 }}
          columns={[
            { title: '标题 / 屏蔽词', dataIndex: 'title', render: (v) => label(v) },
            { title: '类型', dataIndex: 'kind', render: (v) => label(v) },
            {
              title: '状态',
              render: (_, r) => (
                <Tag color={r.active ? 'success' : 'default'}>{r.active ? '启用' : '停用'}</Tag>
              ),
            },
            { title: '排序', dataIndex: 'sort_order' },
            {
              title: '操作',
              render: (_, r) => (
                <Button
                  type="link"
                  onClick={() => {
                    setItem(r)
                    setOpen(true)
                  }}
                >
                  编辑
                </Button>
              ),
            },
          ]}
        />
      </Card>
      {open && (
        <Drawer
          open
          size="large"
          title={item ? '编辑平台资源' : '新建平台资源'}
          onClose={() => setOpen(false)}
        >
          <ResourceEditor
            key={item?.id ?? 'global'}
            item={item}
            save={async (v) => {
              await workspace.globalResource(v)
              await q.refetch()
              setOpen(false)
            }}
          />
        </Drawer>
      )}
      {q.error && <Alert type="error" showIcon title={q.error.message} />}
    </section>
  )
}

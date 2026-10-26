import { formatDate } from '../../../shared/format-date'
import { PaginationControls } from '../../../shared/pagination-controls'
import { useDialogs } from '../../../shared/use-dialogs'
import { Disclosure } from '../../../shared/disclosure'
import { useSearchParams } from 'react-router-dom';
import { AppShell, navigation, schoolRoleLabel } from '../../../app/shell';
import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Card, Input, Select, Space, Tag , Empty } from 'antd';
import MobileDelivery, { ExecutionDevices } from '../../operations/mobile-delivery'
import BroadcastTasks from '../../operations/broadcast-tasks'
import { Directory } from './directory';
import { Assets, GroupPicker } from './assets';
import { Members } from './members';
import { Editor } from './editor';
import { Accounts, Schools } from './platform';
import { workspace, type Row, type Json, type Resource, type School, type User } from './api';
export { label } from '../../../shared/label'
import { label } from '../../../shared/label'
import { SessionGate } from '../../identity'
import { useSchools } from '../../school-context'
export { SessionGate } from '../../identity'
const states: Record<string, string> = { member: '普通成员', operator: '运营', admin: '学校管理员', revoked: '已撤销学校授权', pending: '待审核', approved: '已通过', rejected: '已拒绝', visible: '展示中', hidden: '已隐藏', deleted: '已删除', prepared: '等待客户端结果', client_reported_success: '客户端报告成功', client_reported_failure: '客户端报告失败', unknown: '结果不确定', cancelled: '已取消', operator_confirmed: '人工确认已发送', operator_rejected: '人工确认未发送', open: '待处理', resolved: '已处理', dismissed: '已驳回' };
export const stateLabel = (v: Json | undefined) => states[label(v)] ?? label(v);
export function Workspace() { return <SessionGate showSessionBar={false}>{(user, logout) => <Admin user={user} logout={logout}/>}</SessionGate>; }
function Admin({ user, logout }: { user: User; logout: () => Promise<void> }) {
    const cache = useQueryClient();
    const q = useSchools();
    const [params, setParams] = useSearchParams();
    const selected = params.get('school') ?? '';
    const visibleSchools = q.data?.filter(s => user.platform_admin || s.active);
    const school = visibleSchools?.find(s => s.id === (selected || visibleSchools[0]?.id));
    const role = school?.active ? user.grants.find(g => g.school_id === school.id)?.role : undefined;
    const allowedKeys = navigation.filter(n => user.platform_admin || (n.group !== '平台管理' && (['members', 'settings'].includes(n.key) ? role === 'admin' : role === 'admin' || role === 'operator'))).map(n => n.key);
    const oldTab = params.get('view') ?? 'posts';
    const requestedTab = ['delivery-drafts','delivery-attempts'].includes(oldTab) ? 'broadcast-tasks' : oldTab;
    const tab = allowedKeys.includes(requestedTab) ? requestedTab : allowedKeys[0] ?? '';
    const navigate = (key: string) => setParams(previous => { const next = new URLSearchParams(previous); next.set('view', key); return next; });
    const enterSchool = (id: string, view: string) => setParams({ school: id, view });
    const platformView = navigation.find(n => n.key === tab)?.group === '平台管理';
    const refresh = async () => { await cache.invalidateQueries({ queryKey: ['schools'] }); await cache.invalidateQueries({ queryKey: ['business'] }); };
    return <AppShell active={tab} onNavigate={navigate} username={user.display_name} platformAdmin={user.platform_admin} allowedKeys={allowedKeys} schoolRole={role} onLogout={logout}>
      {q.error && <Alert type="error" title={q.error.message} showIcon/>}
      {!platformView && <div className="scope-toolbar"><div><span className="field-caption">当前学校</span><Select aria-label="当前学校" className="school-select" loading={q.isPending} value={school?.id} options={visibleSchools?.map(s => ({ value: s.id, label: s.name }))} placeholder="选择学校" onChange={async (s) => { await cache.cancelQueries({ queryKey: ['business'] }); cache.removeQueries({ queryKey: ['business'] }); setParams(previous => { const next = new URLSearchParams(previous); next.set('school', s); return next; }); }}/></div><span className="scope-note">{school ? '所有业务操作均限定在当前学校' : '选择学校后开始管理业务'}</span>{school && <span>当前权限：{user.platform_admin ? '平台管理员' : schoolRoleLabel(role)}</span>}</div>}
      {tab === 'execution-devices' && user.platform_admin ? <ExecutionDevices/> : tab === 'wecom-assets' && user.platform_admin ? <Assets/> : tab === 'wecom-directory' && user.platform_admin ? <Directory/>
        : tab === 'schools' && user.platform_admin ? <Schools schools={q.data ?? []} loading={q.isPending} refresh={refresh} enter={enterSchool}/>
        : (tab === 'accounts' || tab === 'administrators') && user.platform_admin ? <Accounts key={tab} administrators={tab === 'administrators'} user={user} schools={q.data ?? []}/>
        : tab === 'platform-resources' && user.platform_admin ? <Card title="平台默认展示配置"><GlobalResources /></Card>
        : q.isPending ? <Empty className="empty-state" description={<>正在加载学校…</>} />
        : !school ? <Empty className="empty-state" description={<><h2>尚未选择学校</h2><p>{user.platform_admin ? '请在「学校管理」中创建学校，或选择已有学校。' : '请联系管理员为账号分配学校权限。'}</p>{user.platform_admin && <Button type="primary" onClick={() => navigate('schools')}>创建学校</Button>}</>} />
        : !tab ? <Alert type="info" title="当前账号没有该学校的后台管理权限，请联系平台管理员。"/>
        : <div className="business-panel">{tab === 'mobile-delivery' ? <MobileDelivery key={school.id} school={school.id} role={role}/> : tab === 'broadcast-tasks' ? <BroadcastTasks key={school.id} school={school.id} admin={role === 'admin'} platformAdmin={user.platform_admin}/> : tab === 'members' ? <Members key={school.id} school={school} platformAdmin={user.platform_admin} refresh={refresh}/> : tab === 'settings' ? <Card title={`${school.name} · 学校设置`}><Settings key={school.id} school={school} refresh={refresh}/></Card> : <Panel platformAdmin={user.platform_admin} key={`${school.id}:${tab}`} schoolAdmin={user.platform_admin || role === 'admin'} school={school} resource={tab as Resource} refresh={refresh}/>}</div>}
    </AppShell>;
}
function Settings({ school, refresh }: {
    school: School;
    refresh: () => Promise<void>;
}) { return <Editor fields={[{ name: 'name', title: '学校名称', value: school.name }, { name: 'post_audit_mode', title: '帖子审核模式', value: label(school.config.post_audit_mode) || 'manual', options: [{ value: 'manual', label: '人工审核' }, { value: 'auto', label: '自动通过' }] }, { name: 'comment_audit_mode', title: '评论审核模式', value: label(school.config.comment_audit_mode) || 'manual', options: [{ value: 'manual', label: '人工审核' }, { value: 'auto', label: '自动通过' }] }, { name: 'theme_color', title: '主题颜色', type: 'color', value: label(school.config.theme_color) || '#25624b' }]} onSave={async (v) => { await workspace.updateSchool(school.id, { name: v.name, revision: school.revision, config: { ...school.config, post_audit_mode: v.post_audit_mode, comment_audit_mode: v.comment_audit_mode, theme_color: v.theme_color } }); await refresh(); }}/>; }
function Panel({ school, resource, refresh, schoolAdmin, platformAdmin }: {
    platformAdmin: boolean;
    schoolAdmin: boolean;
    school: School;
    resource: Resource;
    refresh: () => Promise<void>;
}) {
    const s = school.id;
    const [page, setPage] = useState(1), [search, setSearch] = useState(''), [status, setStatus] = useState('');
    const [error, setError] = useState(''), [busy, setBusy] = useState(false), [editing, setEditing] = useState<Row>();
    const query = `?${new URLSearchParams({ 'page[number]': String(page), 'page[size]': '30', ...(search ? { search } : {}), ...(status ? { status } : {}) })}`;
    const q = useQuery({ queryKey: ['business', s, resource, query], queryFn: ({ signal }) => workspace.list(s, resource, query, signal) });
    const dialogs = useDialogs();
    async function act(path: string, body: unknown, method = 'POST') { setBusy(true); setError(''); try {
        await workspace.mutate(s, path, body, method);
        await refresh();
    }
    catch (e) {
        setError((e as Error).message);
    }
    finally {
        setBusy(false);
    } }
    const mutation = async (path: string, body: unknown, method = 'POST') => { await workspace.mutate(s, path, body, method); await refresh(); };
    return <>{dialogs.contextHolder}<div className="list-toolbar"><Space wrap><Input.Search aria-label="搜索内容" placeholder="搜索标题或正文" onSearch={v => { setSearch(v); setPage(1); }}/>{['posts', 'comments'].includes(resource) && <Select aria-label="审核状态" value={status} style={{ width: 140 }} onChange={setStatus} options={[{ value: '', label: '全部状态' }, ...['pending', 'approved', 'rejected'].map(value => ({ value, label: states[value] }))]}/>}<Button onClick={() => q.refetch()} loading={q.isFetching}>刷新</Button></Space><span className="list-count">{q.data ? `本页 ${q.data.items.length} 条记录` : '正在获取记录'}</span></div>{(error || q.error) && <Alert type="error" title={error || q.error?.message}/>}
 {resource === 'resources' && schoolAdmin && <ResourceEditor key={editing?.id ?? 'new'} item={editing} save={v => mutation('resources', v)}/>}
 {resource === 'groups' && schoolAdmin && !editing && <GroupPicker key={school.id} school={school.id} onSaved={refresh}/>}
 {resource === 'groups' && schoolAdmin && (editing || platformAdmin) && <Disclosure title={<>{editing ? '编辑学校群' : '平台例外：手填已知群标识'}</>} open={!!editing}><Editor key={editing?.id ?? 'new'} fields={[{ name: 'name', title: '群名称', value: label(editing?.name) }, { name: 'corp_id', title: '企业 ID（与应用配置一致）', readOnly: !!editing, value: label(editing?.corp_id) }, { name: 'chat_id', title: '客户群 ID（保存后不可更换）', readOnly: !!editing, value: label(editing?.chat_id) }, { name: 'active', title: '启用状态', value: editing ? String(editing.active) : 'true', options: [{ value: 'true', label: '启用' }, { value: 'false', label: '撤销' }] }]} onSave={v => mutation('groups', { ...v, id: editing?.id, revision: editing?.revision ?? 0, active: v.active === 'true' })}/></Disclosure>}
 {resource === 'templates' && <Editor fields={[{ name: 'name', title: '模板名称' }]} onSave={v => mutation('templates', v)}/>}
 {q.isPending && <Alert type="info" role="status"  title={<>正在加载…</>} />}{q.data?.items.length === 0 && <Empty className="empty-state" description={<><h3>暂无记录</h3><p>当前学校下没有符合条件的记录。</p></>} />}
 <div className="record-list">{q.data?.items.map(r => <Card key={r.id} size="small" title={label(r.title ?? r.name ?? r.group_name ?? r.display_name ?? r.action) || r.id} extra={<Tag>{stateLabel(r.audit_status ?? r.state ?? r.role ?? r.kind)}</Tag>}>
 {resource === 'members' && <p>{school.name} · {label(r.username)} · {schoolRoleLabel(label(r.role))}</p>}
 {r.content && <p className="text-content">{label(r.content)}</p>}{r.body && <p className="text-content">{label(r.body)}</p>}{r.contact && <p>联系方式：{label(r.contact)}</p>}{r.visibility && <Tag>{stateLabel(r.visibility)}</Tag>}{r.reconciliation && <p>核对结果：{stateLabel(r.reconciliation)} · {label(r.reconciliation_note)}</p>}<small>{formatDate(r.created_at)} {r.id}</small>
 {r.detail && <pre>{label(r.detail)}</pre>}{r.url && <p><a href={label(r.url)} target="_blank" rel="noreferrer">查看资源链接</a></p>}
 {['posts', 'comments'].includes(resource) && <Space wrap>{['approved', 'rejected', 'pending'].map(decision => <Button key={decision} disabled={busy} onClick={async () => { const reason = await dialogs.prompt('审核说明'); if (reason !== null)
            void act(`${resource}/${r.id}/moderation-decisions`, { revision: r.revision, decision, reason }); }}>{states[decision]}</Button>)}{resource === 'posts' && <>{['visible', 'hidden', 'deleted'].map(visibility => <Button key={visibility} disabled={busy} onClick={() => act(`posts/${r.id}/visibility`, { revision: r.revision, visibility, top_sort: r.top_sort ?? 0 }, 'PUT')}>{states[visibility]}</Button>)}<Button disabled={busy} onClick={() => act(`posts/${r.id}/visibility`, { revision: r.revision, visibility: r.visibility, top_sort: r.top_sort ? 0 : 100 }, 'PUT')}>{r.top_sort ? '取消置顶' : '置顶'}</Button></>}</Space>}
 {resource === 'cases' && r.state === 'open' && <Editor fields={[{ name: 'resolution', title: '处理说明' }, { name: 'state', title: '处理结果', options: [{ value: 'resolved', label: '已处理' }, { value: 'dismissed', label: '驳回' }] }]} onSave={v => mutation(`cases/${r.id}`, { ...v, revision: r.revision }, 'PUT')}/>}
 {['resources', 'groups'].includes(resource) && schoolAdmin && <Button onClick={() => setEditing(r)}>编辑</Button>}
 {resource === 'templates' && <><Editor fields={[{ name: 'body', title: '新版本正文（支持 {{school.name}}、{{post.title}}、{{post.summary}}、{{post.detail_url}}）', type: 'textarea' }]} onSave={v => mutation(`templates/${r.id}/versions`, v)}/>{Array.isArray(r.versions) && r.versions.map(v => { const version = v as Row; return <div key={version.id}><pre>{label(version.body)}</pre>{version.published ? <Tag>已发布 · {version.id}</Tag> : <Button disabled={busy} onClick={() => act(`template-versions/${version.id}/publish`, { revision: version.revision })}>发布此版本</Button>}</div>; })}</>}
 {resource === 'delivery-drafts' && <a href={`/wecom?school=${s}&draft=${r.id}`}>预览草稿与客户群入口说明 ↗</a>}
 {resource === 'delivery-attempts' && schoolAdmin && <><p>客户端结果与人工核对独立保存；不确定结果不会自动重发。</p><Editor fields={[{ name: 'decision', title: '核对结论', options: [{ value: 'operator_confirmed', label: '人工确认已发送' }, { value: 'operator_rejected', label: '人工确认未发送' }] }, { name: 'reason', title: '核对依据', type: 'textarea' }]} onSave={v => mutation(`delivery-attempts/${r.id}/reconciliations`, v)}/></>}
 </Card>)}</div>{['posts', 'comments', 'audit', 'delivery-drafts', 'delivery-attempts', 'cases', 'notifications'].includes(resource) && <PaginationControls page={page} more={q.data?.page?.has_more} busy={q.isFetching} change={setPage} />}
 </>;
}
function ResourceEditor({ item, save }: {
    item?: Row;
    save: (v: unknown) => Promise<unknown>;
}) { return <Editor fields={[{ name: 'kind', title: '类型', value: label(item?.kind), options: [['banner', '轮播'], ['qrcode', '二维码'], ['service', '客服'], ['legal', '法律文本'], ['tag', '标签'], ['blocked_word', '屏蔽词']].map(([value, label]) => ({ value, label })) }, { name: 'title', title: '标题或屏蔽词', value: label(item?.title) }, { name: 'content', title: '正文 / 说明', type: 'textarea', optional: true, value: label(item?.content) }, { name: 'url', title: '图片或跳转链接', optional: true, value: label(item?.url) }, { name: 'sort_order', title: '排序', type: 'number', value: label(item?.sort_order) || 0 }, { name: 'starts_at', title: '生效时间', type: 'datetime', optional: true, value: label(item?.starts_at) }, { name: 'ends_at', title: '失效时间', type: 'datetime', optional: true, value: label(item?.ends_at) }, { name: 'active', title: '状态', value: item ? String(item.active) : 'true', options: [{ value: 'true', label: '启用' }, { value: 'false', label: '停用' }] }]} onSave={v => save({ ...v, id: item?.id, revision: item?.revision ?? 0, active: v.active === 'true', sort_order: Number(v.sort_order), starts_at: v.starts_at || null, ends_at: v.ends_at || null })}/>; }
function GlobalResources() { const q = useQuery({ queryKey: ['global-resources'], queryFn: workspace.globalResources }); const [item, setItem] = useState<Row>(); return <section><h3>平台默认展示配置</h3><p>学校有同类启用配置时优先使用学校配置；全局屏蔽词始终生效。</p><ResourceEditor key={item?.id ?? 'global'} item={item} save={async (v) => { await workspace.globalResource(v); await q.refetch(); }}/>{q.data?.map(r => <p key={r.id}>{label(r.title)} <Button onClick={() => setItem(r)}>编辑</Button></p>)}{q.error && <Alert type="error" showIcon   title={<>{q.error.message}</>} />}</section>; }

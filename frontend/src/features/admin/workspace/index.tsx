import { useSearchParams } from 'react-router-dom';
import { AppShell, navigation } from '../../../app/shell';
import { useState, type FormEvent, type ReactNode } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Card, Input, Select, Space, Tag } from 'antd';
import { identity, workspace, type Row, type Json, type Resource, type School, type User } from './api';
export const label = (v: Json | undefined) => v === null || v === undefined ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v);
const states: Record<string, string> = { pending: '待审核', approved: '已通过', rejected: '已拒绝', visible: '展示中', hidden: '已隐藏', deleted: '已删除', prepared: '等待客户端结果', client_reported_success: '客户端报告成功', client_reported_failure: '客户端报告失败', unknown: '结果不确定', cancelled: '已取消', operator_confirmed: '人工确认已发送', operator_rejected: '人工确认未发送', open: '待处理', resolved: '已处理', dismissed: '已驳回' };
export const stateLabel = (v: Json | undefined) => states[label(v)] ?? label(v);
export function SessionGate({ children, wecom = false, showSessionBar = true }: {
    children: (user: User, logout: () => Promise<void>) => ReactNode;
    showSessionBar?: boolean;
    wecom?: boolean;
}) {
    const cache = useQueryClient();
    const q = useQuery({ queryKey: ['session'], queryFn: identity.session, retry: false });
    const [error, setError] = useState('');
    const [busy, setBusy] = useState(false);
    async function login(e: FormEvent<HTMLFormElement>) { e.preventDefault(); const f = new FormData(e.currentTarget); setBusy(true); try {
        const s = await identity.login(String(f.get('username')), String(f.get('password')));
        await cache.cancelQueries();
        cache.clear();
        cache.setQueryData(['session'], s);
    }
    catch (e) {
        setError((e as Error).message);
    }
    finally {
        setBusy(false);
    } }
    async function logout() {
        setError('');
        try { await identity.logout(); await cache.cancelQueries(); cache.clear(); }
        catch (e) { setError((e as Error).message); }
    }
    if (q.isPending) return <div className="loading" role="status">正在确认登录状态…</div>;
    if (!q.data) return <div className="auth-page"><section className="auth-panel">
        <div className="auth-brand"><span className="brand-mark">W</span> WeKeeper</div><div><h1>登录工作空间</h1><p>校园社区 · 内容运营 · 人工分享</p></div>
        {error && <Alert type="error" title={error} showIcon/>}<form onSubmit={login} className="business-form"><label>账号<input name="username" placeholder="请输入账号" autoComplete="username" required maxLength={100}/></label><label>密码<input name="password" placeholder="请输入密码" type="password" autoComplete="current-password" required maxLength={128}/></label><button className="primary-button" disabled={busy}>{busy ? '正在登录…' : '登录'}</button></form>{wecom && <a href="/api/v1/wecom/oauth/start">使用企业微信身份登录</a>}
        <p className="auth-caption">使用管理员分配的账号进入工作空间</p>
    </section></div>;
    return <>{showSessionBar && <div className="session-bar"><span>{q.data.user.display_name}</span><Button onClick={logout}>退出登录</Button></div>}{error && <div className="session-error" role="alert">{error}</div>}{children(q.data.user, logout)}</>;
}

interface Field {
    name: string;
    title: string;
    type?: string;
    options?: {
        value: string;
        label: string;
    }[];
    optional?: boolean;
    readOnly?: boolean;
    value?: string | number;
}
function Editor({ fields, onSave, submit = '保存' }: {
    fields: Field[];
    onSave: (v: Record<string, string>) => Promise<unknown>;
    submit?: string;
}) {
    const [busy, setBusy] = useState(false), [error, setError] = useState(''), [done, setDone] = useState(false);
    async function save(e: FormEvent<HTMLFormElement>) { e.preventDefault(); const form = e.currentTarget; const v = Object.fromEntries(new FormData(form).entries()) as Record<string, string>; setBusy(true); setError(''); setDone(false); try {
        await onSave(v);
        setDone(true);
    }
    catch (e) {
        setError((e as Error).message);
    }
    finally {
        setBusy(false);
    } }
    return <form onSubmit={save} className="business-form">{fields.map(f => <label key={f.name}>{f.title}{f.type === 'textarea' ? <textarea name={f.name} required={!f.optional} defaultValue={f.value} rows={4}/> : f.options ? <select name={f.name} required={!f.optional} defaultValue={f.value}><option value="">请选择</option>{f.options.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select> : <input readOnly={f.readOnly} name={f.name} type={f.type ?? 'text'} required={!f.optional} defaultValue={f.value}/>}</label>)}{error && <Alert type="error" title={error}/>}{done && <p role="status">已保存</p>}<button disabled={busy}>{busy ? '正在保存…' : submit}</button></form>;
}
export function Workspace() { return <SessionGate showSessionBar={false}>{(user, logout) => <Admin user={user} logout={logout}/>}</SessionGate>; }
function Admin({ user, logout }: { user: User; logout: () => Promise<void> }) {
    const cache = useQueryClient();
    const q = useQuery({ queryKey: ['schools'], queryFn: workspace.schools });
    const [params, setParams] = useSearchParams();
    const selected = params.get('school') ?? '';
    const school = q.data?.find(s => s.id === (selected || q.data[0]?.id));
    const requestedTab = params.get('view') ?? 'posts';
    const tab = navigation.some(n => n.key === requestedTab && (n.group !== '平台管理' || user.platform_admin)) ? requestedTab : 'posts';
    const navigate = (key: string) => setParams(previous => { const next = new URLSearchParams(previous); next.set('view', key); return next; });
    const refresh = async () => { await cache.invalidateQueries({ queryKey: ['schools'] }); await cache.invalidateQueries({ queryKey: ['business'] }); };
    return <AppShell active={tab} onNavigate={navigate} username={user.display_name} platformAdmin={user.platform_admin} onLogout={logout}>
      {q.error && <Alert type="error" title={q.error.message} showIcon/>}
      <div className="scope-toolbar"><div><span className="field-caption">当前学校</span><Select aria-label="当前学校" className="school-select" loading={q.isPending} value={school?.id} options={q.data?.map(s => ({ value: s.id, label: s.name }))} placeholder="选择学校" onChange={async (s) => { await cache.cancelQueries({ queryKey: ['business'] }); cache.removeQueries({ queryKey: ['business'] }); setParams(previous => { const next = new URLSearchParams(previous); next.set('school', s); return next; }); }}/></div><span className="scope-note">{school ? '所有业务操作均限定在当前学校' : '选择学校后开始管理业务'}</span></div>
      {tab === 'accounts' && user.platform_admin ? <div className="management-stack"><Card title="新建学校"><Editor fields={[{ name: 'name', title: '学校名称' }]} onSave={async v => { await workspace.createSchool(v.name); await refresh(); }}/></Card><Card title="账号管理"><Accounts /></Card></div>
        : tab === 'platform-resources' && user.platform_admin ? <Card title="平台默认展示配置"><GlobalResources /></Card>
        : q.isPending ? <div className="empty-state" role="status">正在加载学校…</div>
        : !school ? <div className="empty-state"><span className="empty-mark">◇</span><h2>尚未选择学校</h2><p>{user.platform_admin ? '请在「学校与账号」中创建学校，或选择已有学校。' : '请联系管理员为账号分配学校权限。'}</p>{user.platform_admin && <Button type="primary" onClick={() => navigate('accounts')}>创建学校</Button>}</div>
        : <div className="business-panel">{tab === 'settings' ? <Card title="基础设置"><Settings key={school.id} school={school} refresh={refresh}/></Card> : <Panel key={`${school.id}:${tab}`} platformAdmin={user.platform_admin} school={school} resource={tab as Resource} refresh={refresh}/>}</div>}
    </AppShell>;
}
function Accounts() { const cache = useQueryClient(); const q = useQuery({ queryKey: ['accounts'], queryFn: identity.accounts }); const [editing, setEditing] = useState<Row>(); const refresh = () => cache.invalidateQueries({ queryKey: ['accounts'] }); return <section><h3>账号</h3><Editor fields={[{ name: 'username', title: '账号' }, { name: 'display_name', title: '昵称' }, { name: 'password', title: '初始密码（至少12个字符）', type: 'password' }]} onSave={async (v) => { await identity.create({ username: v.username, display_name: v.display_name, password: v.password }); await refresh(); }}/>{q.error && <p role="alert">{q.error.message}</p>}{q.data?.map(r => <p key={r.id}>{label(r.display_name)} · {label(r.username)} <code>{r.id}</code><Button onClick={() => setEditing(r)}>管理</Button></p>)}{editing && <><Editor key={editing.id} fields={[{ name: 'display_name', title: '昵称', value: label(editing.display_name) }, { name: 'wecom_userid', title: '企业微信成员 UserID', optional: true, value: label(editing.wecom_userid) }, { name: 'active', title: '账号状态', value: String(editing.active), options: [{ value: 'true', label: '启用' }, { value: 'false', label: '禁用' }] }]} onSave={async (v) => { await identity.update(editing.id, { ...v, active: v.active === 'true', wecom_userid: v.wecom_userid || null }); await refresh(); }}/><Editor fields={[{ name: 'password', title: '重置密码（撤销该账号所有会话）', type: 'password' }]} onSave={v => identity.reset(editing.id, v.password)}/></>}</section>; }
function Settings({ school, refresh }: {
    school: School;
    refresh: () => Promise<void>;
}) { return <Editor fields={[{ name: 'name', title: '学校名称', value: school.name }, { name: 'post_audit_mode', title: '帖子审核模式', value: label(school.config.post_audit_mode) || 'manual', options: [{ value: 'manual', label: '人工审核' }, { value: 'auto', label: '自动通过' }] }, { name: 'comment_audit_mode', title: '评论审核模式', value: label(school.config.comment_audit_mode) || 'manual', options: [{ value: 'manual', label: '人工审核' }, { value: 'auto', label: '自动通过' }] }, { name: 'theme_color', title: '主题颜色', type: 'color', value: label(school.config.theme_color) || '#25624b' }]} onSave={async (v) => { await workspace.updateSchool(school.id, { name: v.name, revision: school.revision, config: { ...school.config, post_audit_mode: v.post_audit_mode, comment_audit_mode: v.comment_audit_mode, theme_color: v.theme_color } }); await refresh(); }}/>; }
function Panel({ school, resource, refresh, platformAdmin }: {
    platformAdmin: boolean;
    school: School;
    resource: Resource;
    refresh: () => Promise<void>;
}) {
    const s = school.id;
    const [page, setPage] = useState(1), [search, setSearch] = useState(''), [status, setStatus] = useState('');
    const [error, setError] = useState(''), [busy, setBusy] = useState(false), [editing, setEditing] = useState<Row>();
    const query = `?${new URLSearchParams({ page: String(page), page_size: '30', ...(search ? { search } : {}), ...(status ? { status } : {}) })}`;
    const q = useQuery({ queryKey: ['business', s, resource, query], queryFn: ({ signal }) => workspace.list(s, resource, query, signal) });
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
    return <><div className="list-toolbar"><Space wrap><Input.Search aria-label="搜索内容" placeholder="搜索标题或正文" onSearch={v => { setSearch(v); setPage(1); }}/>{['posts', 'comments'].includes(resource) && <Select aria-label="审核状态" value={status} style={{ width: 140 }} onChange={setStatus} options={[{ value: '', label: '全部状态' }, ...['pending', 'approved', 'rejected'].map(value => ({ value, label: states[value] }))]}/>}<Button onClick={() => q.refetch()} loading={q.isFetching}>刷新</Button></Space><span className="list-count">{q.data ? `本页 ${q.data.length} 条记录` : '正在获取记录'}</span></div>{(error || q.error) && <Alert type="error" title={error || q.error?.message}/>}
 {resource === 'resources' && <ResourceEditor key={editing?.id ?? 'new'} item={editing} save={v => mutation('resources', v)}/>}
 {resource === 'groups' && <Editor key={editing?.id ?? 'new'} fields={[{ name: 'name', title: '群名称', value: label(editing?.name) }, { name: 'corp_id', title: '企业 ID（与应用配置一致）', readOnly: !!editing, value: label(editing?.corp_id) }, { name: 'chat_id', title: '客户群 ID（保存后不可更换）', readOnly: !!editing, value: label(editing?.chat_id) }, { name: 'active', title: '启用状态', value: editing ? String(editing.active) : 'true', options: [{ value: 'true', label: '启用' }, { value: 'false', label: '撤销' }] }]} onSave={v => mutation('groups', { ...v, id: editing?.id, revision: editing?.revision ?? 0, active: v.active === 'true' })}/>}
 {resource === 'members' && (platformAdmin ? <MemberGrant school={s} refresh={refresh}/> : <p>学校授权由平台管理员维护。</p>)}
 {resource === 'templates' && <Editor fields={[{ name: 'name', title: '模板名称' }]} onSave={v => mutation('templates', v)}/>}
 {resource === 'delivery-drafts' && <DraftEditor school={s} save={v => mutation('delivery-drafts', v)}/>}
 {q.isPending && <p role="status">正在加载…</p>}{q.data?.length === 0 && <div className="empty-state"><span className="empty-mark">◇</span><h3>暂无记录</h3><p>当前学校下没有符合条件的记录。</p></div>}
 <div className="record-list">{q.data?.map(r => <Card key={r.id} size="small" title={label(r.title ?? r.name ?? r.group_name ?? r.display_name ?? r.action) || r.id} extra={<Tag>{stateLabel(r.audit_status ?? r.state ?? r.role ?? r.kind)}</Tag>}>
 {r.content && <p className="text-content">{label(r.content)}</p>}{r.body && <p className="text-content">{label(r.body)}</p>}{r.contact && <p>联系方式：{label(r.contact)}</p>}{r.visibility && <Tag>{stateLabel(r.visibility)}</Tag>}{r.reconciliation && <p>核对结果：{stateLabel(r.reconciliation)} · {label(r.reconciliation_note)}</p>}<small>{label(r.created_at)} {r.id}</small>
 {r.detail && <pre>{label(r.detail)}</pre>}{r.url && <p><a href={label(r.url)} target="_blank" rel="noreferrer">查看资源链接</a></p>}
 {['posts', 'comments'].includes(resource) && <Space wrap>{['approved', 'rejected', 'pending'].map(decision => <Button key={decision} disabled={busy} onClick={() => { const reason = window.prompt('审核说明'); if (reason !== null)
            void act(`${resource}/${r.id}/moderation-decisions`, { revision: r.revision, decision, reason }); }}>{states[decision]}</Button>)}{resource === 'posts' && <>{['visible', 'hidden', 'deleted'].map(visibility => <Button key={visibility} disabled={busy} onClick={() => act(`posts/${r.id}/visibility`, { revision: r.revision, visibility, top_sort: r.top_sort ?? 0 }, 'PUT')}>{states[visibility]}</Button>)}<Button disabled={busy} onClick={() => act(`posts/${r.id}/visibility`, { revision: r.revision, visibility: r.visibility, top_sort: r.top_sort ? 0 : 100 }, 'PUT')}>{r.top_sort ? '取消置顶' : '置顶'}</Button></>}</Space>}
 {resource === 'cases' && r.state === 'open' && <Editor fields={[{ name: 'resolution', title: '处理说明' }, { name: 'state', title: '处理结果', options: [{ value: 'resolved', label: '已处理' }, { value: 'dismissed', label: '驳回' }] }]} onSave={v => mutation(`cases/${r.id}`, { ...v, revision: r.revision }, 'PUT')}/>}
 {['resources', 'groups'].includes(resource) && <Button onClick={() => setEditing(r)}>编辑</Button>}
 {resource === 'templates' && <><Editor fields={[{ name: 'body', title: '新版本正文（支持 {{school.name}}、{{post.title}}、{{post.summary}}、{{post.detail_url}}）', type: 'textarea' }]} onSave={v => mutation(`templates/${r.id}/versions`, v)}/>{Array.isArray(r.versions) && r.versions.map(v => { const version = v as Row; return <div key={version.id}><pre>{label(version.body)}</pre>{version.published ? <Tag>已发布 · {version.id}</Tag> : <Button disabled={busy} onClick={() => act(`template-versions/${version.id}/publish`, { revision: version.revision })}>发布此版本</Button>}</div>; })}</>}
 {resource === 'delivery-drafts' && <a href={`/wecom?school=${s}&draft=${r.id}`}>预览草稿与客户群入口说明 ↗</a>}
 {resource === 'delivery-attempts' && <><p>客户端结果与人工核对独立保存；不确定结果不会自动重发。</p><Editor fields={[{ name: 'decision', title: '核对结论', options: [{ value: 'operator_confirmed', label: '人工确认已发送' }, { value: 'operator_rejected', label: '人工确认未发送' }] }, { name: 'reason', title: '核对依据', type: 'textarea' }]} onSave={v => mutation(`delivery-attempts/${r.id}/reconciliations`, v)}/></>}
 </Card>)}</div>{['posts', 'comments', 'audit', 'delivery-drafts', 'delivery-attempts'].includes(resource) && <Space><Button disabled={page === 1} onClick={() => setPage(page - 1)}>上一页</Button><span>第 {page} 页</span><Button disabled={(q.data?.length ?? 0) < 30} onClick={() => setPage(page + 1)}>下一页</Button></Space>}
 </>;
}
function MemberGrant({ school, refresh }: { school: string; refresh: () => Promise<void> }) {
    const accounts = useQuery({ queryKey: ['accounts'], queryFn: identity.accounts });
    const [selected, setSelected] = useState<string[]>([]), [role, setRole] = useState('operator'), [busy, setBusy] = useState(false), [error, setError] = useState(''), [done, setDone] = useState('');
    async function save() {
        setBusy(true); setError(''); setDone('');
        const pending = [...selected];
        try {
            for (const id of selected) {
                await workspace.mutate(school, `members/${id}`, { role }, 'PUT');
                pending.splice(pending.indexOf(id), 1);
            }
            setDone('学校授权已保存');
        } catch (e) { setError(`部分操作未完成，已保留待处理账号：${(e as Error).message}`); }
        finally { setSelected(pending); setBusy(false); await refresh(); }
    }
    return <section><p>按姓名或账号搜索，可一次选择多名成员。这里展示本地账号及企微绑定状态。</p>
        {accounts.error && <p role="alert">账号加载失败：{accounts.error.message}<button onClick={() => void accounts.refetch()}>重新加载账号</button></p>}
        <Select aria-label="选择学校成员" mode="multiple" showSearch={{ optionFilterProp: 'label' }} loading={accounts.isPending} disabled={busy} value={selected} onChange={setSelected} options={accounts.data?.map(a => ({ value: a.id, label: `${label(a.display_name)} · ${label(a.username)} · ${a.wecom_userid ? '已绑定企微' : '未绑定企微'}${a.active ? '' : ' · 已禁用'}` }))}/>
        <label>学校角色<select disabled={busy} value={role} onChange={e => setRole(e.target.value)}><option value="member">普通成员</option><option value="operator">运营</option><option value="admin">学校管理员</option><option value="revoked">撤销学校授权</option></select></label>
        <button disabled={busy || !selected.length} onClick={() => void save()}>保存所选成员授权</button>{error && <p role="alert">{error}</p>}{done && <p role="status">{done}</p>}
    </section>;
}
function ResourceEditor({ item, save }: {
    item?: Row;
    save: (v: unknown) => Promise<unknown>;
}) { return <Editor fields={[{ name: 'kind', title: '类型', value: label(item?.kind), options: [['banner', '轮播'], ['qrcode', '二维码'], ['service', '客服'], ['legal', '法律文本'], ['tag', '标签'], ['blocked_word', '屏蔽词']].map(([value, label]) => ({ value, label })) }, { name: 'title', title: '标题或屏蔽词', value: label(item?.title) }, { name: 'content', title: '正文 / 说明', type: 'textarea', optional: true, value: label(item?.content) }, { name: 'url', title: '图片或跳转链接', optional: true, value: label(item?.url) }, { name: 'sort_order', title: '排序', type: 'number', value: label(item?.sort_order) || 0 }, { name: 'starts_at', title: '生效时间（带时区，如 2026-10-20T08:00:00+08:00）', optional: true, value: label(item?.starts_at) }, { name: 'ends_at', title: '失效时间（同上）', optional: true, value: label(item?.ends_at) }, { name: 'active', title: '状态', value: item ? String(item.active) : 'true', options: [{ value: 'true', label: '启用' }, { value: 'false', label: '停用' }] }]} onSave={v => save({ ...v, id: item?.id, revision: item?.revision ?? 0, active: v.active === 'true', sort_order: Number(v.sort_order), starts_at: v.starts_at || null, ends_at: v.ends_at || null })}/>; }
function DraftEditor({ school, save }: {
    school: string;
    save: (v: unknown) => Promise<unknown>;
}) {
    const groups = useQuery({ queryKey: ['business', school, 'groups'], queryFn: () => workspace.list(school, 'groups') });
    const templates = useQuery({ queryKey: ['business', school, 'templates'], queryFn: () => workspace.list(school, 'templates') });
    const [page, setPage] = useState(1), [search, setSearch] = useState('');
    const posts = useQuery({ queryKey: ['business', school, 'approved-posts', page, search], queryFn: () => workspace.list(school, 'posts', `?status=approved&page_size=30&page=${page}&search=${encodeURIComponent(search)}`) });
    const [chosen, setChosen] = useState<string[]>([]);
    const versions = templates.data?.flatMap(t => Array.isArray(t.versions) ? t.versions.filter(v => (v as Row).published).map(v => ({ value: (v as Row).id, label: `${label(t.name)} · ${(v as Row).id}` })) : []) ?? [];
    return <details open><summary>生成分享草稿</summary><p>选择仍在展示的已审核内容，生成后正文固定。正文上限为 2000 UTF-8 字节。</p><label>搜索已审核内容<input value={search} onChange={e => { setSearch(e.target.value); setPage(1); }}/></label>
    {[groups, templates, posts].map((q, i) => q.error ? <p role="alert" key={i}>{['客户群', '模板', '内容'][i]}加载失败：{q.error.message}<button onClick={() => void q.refetch()}>重新加载</button></p> : q.isPending ? <p role="status" key={i}>正在加载{['客户群', '模板', '内容'][i]}…</p> : null)}
    <p>已选 {chosen.length} 条内容<button onClick={() => setChosen([])}>清空选择</button></p>{posts.data?.filter(p => p.visibility === 'visible').map(p => <label className="check-row" key={p.id}><input type="checkbox" checked={chosen.includes(p.id)} onChange={e => setChosen(e.target.checked ? [...chosen, p.id] : chosen.filter(id => id !== p.id))}/>{label(p.title)}</label>)}<div><button disabled={page === 1} onClick={() => setPage(p => p - 1)}>上一页内容</button><span>第 {page} 页</span><button disabled={(posts.data?.length ?? 0) < 30} onClick={() => setPage(p => p + 1)}>下一页内容</button></div><Editor fields={[{ name: 'group_id', title: '目标客户群', options: groups.data?.filter(g => g.active).map(g => ({ value: g.id, label: label(g.name) })) ?? [] }, { name: 'template_version_id', title: '已发布模板版本', options: versions }]} onSave={v => save({ ...v, post_ids: chosen })} submit="生成并预览草稿"/></details>;
}
function GlobalResources() { const q = useQuery({ queryKey: ['global-resources'], queryFn: workspace.globalResources }); const [item, setItem] = useState<Row>(); return <section><h3>平台默认展示配置</h3><p>学校有同类启用配置时优先使用学校配置；全局屏蔽词始终生效。</p><ResourceEditor key={item?.id ?? 'global'} item={item} save={async (v) => { await workspace.globalResource(v); await q.refetch(); }}/>{q.data?.map(r => <p key={r.id}>{label(r.title)} <Button onClick={() => setItem(r)}>编辑</Button></p>)}{q.error && <p role="alert">{q.error.message}</p>}</section>; }

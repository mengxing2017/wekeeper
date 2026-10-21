import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Card, Input, Modal, Select, Space, Table, Tag } from 'antd';
import { identity, workspace, type Row, type School, type User } from './api';
import { WecomBinding } from './directory';
import { Editor } from './editor';
import { schoolRoleLabel } from '../../../app/shell';

const statusOptions = [{ value: '', label: '全部状态' }, { value: 'true', label: '启用' }, { value: 'false', label: '停用' }];
const value = (row: Row, key: string) => String(row[key] ?? '');

export function Schools({ schools, loading, refresh, enter }: {
    schools: School[]; loading: boolean; refresh: () => Promise<void>; enter: (id: string, view: string) => void;
}) {
    const [search, setSearch] = useState(''), [status, setStatus] = useState(''), [creating, setCreating] = useState(false), [page, setPage] = useState(1), [detail, setDetail] = useState<School>();
    const rows = schools.filter(s => s.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()) && (!status || String(s.active) === status));
    return <Card title="学校列表" extra={<Button type="primary" onClick={() => setCreating(true)}>新建学校</Button>}>
        <Space wrap><Input aria-label="搜索学校" placeholder="搜索学校名称" value={search} onChange={e => { setSearch(e.target.value); setPage(1); }}/><Select aria-label="学校状态筛选" value={status} options={statusOptions} onChange={v => { setStatus(v); setPage(1); }}/></Space>
        {creating && <Modal open title="新建学校" footer={null} onCancel={() => setCreating(false)}><Editor fields={[{ name: 'name', title: '学校名称' }]} submit="创建学校" onSave={async v => { await workspace.createSchool(v.name); await refresh(); setCreating(false); }}/></Modal>}
        <Table<School> rowKey="id" loading={loading} dataSource={rows} scroll={{ x: 600 }} pagination={{ current: page, pageSize: 10, onChange: setPage, showSizeChanger: false, showTotal: total => `共 ${total} 所学校` }} columns={[
            { title: '学校名称', dataIndex: 'name' },
            { title: '状态', render: (_, row) => <Tag>{row.active ? '启用' : '停用'}</Tag> },
            { title: '操作', render: (_, row) => <Space><Button onClick={() => setDetail(row)}>详情</Button><Button onClick={() => enter(row.id, 'settings')}>学校配置</Button><Button onClick={() => enter(row.id, 'members')}>学校成员</Button></Space> },
        ]}/>
        {detail && <Modal open title={`${detail.name} · 学校详情`} footer={null} onCancel={() => setDetail(undefined)}><p>学校 ID：{detail.id}</p><p>状态：{detail.active ? '启用' : '停用'}</p><p>帖子审核：{detail.config.post_audit_mode === 'auto' ? '自动通过' : '人工审核'}</p><p>评论审核：{detail.config.comment_audit_mode === 'auto' ? '自动通过' : '人工审核'}</p><Space><Button onClick={() => enter(detail.id, 'settings')}>学校配置</Button><Button onClick={() => enter(detail.id, 'members')}>学校成员</Button></Space></Modal>}
    </Card>;
}

function AccountSchools({ account, schools }: { account: Row; schools: School[] }) {
    const q = useQuery({
        queryKey: ['business', 'account-grants', account.id, schools.map(s => s.id)],
        queryFn: async ({ signal }) => (await Promise.all(schools.map(async school => {
            const members = await workspace.list(school.id, 'members', '', signal);
            const member = members.items.find(m => m.id === account.id);
            return member ? { school, role: value(member, 'role') } : undefined;
        }))).filter(item => item !== undefined),
    });
    return <section><h3>学校授权</h3><p>一个账号可以在多所学校拥有不同角色。撤销单校授权请进入对应学校的成员管理。</p>
        {q.isPending && <p role="status">正在加载学校授权…</p>}
        {q.error && <Alert type="error" title={q.error.message} action={<Button onClick={() => void q.refetch()}>重试</Button>}/>}
        {q.data?.length === 0 && <p>暂无学校授权</p>}
        {q.data?.map(({ school, role }) => <p key={school.id}>{school.name} · {schoolRoleLabel(role)}{!school.active && ' · 学校已停用'}</p>)}
        <p>绑定学校：{schools.find(s => s.id === account.bound_school_id)?.name ?? (account.bound_school_id ? '学校不可用' : '未绑定')}（与学校授权分别维护）</p>
    </section>;
}

export function Accounts({ administrators, user, schools }: {
    administrators: boolean; user: User; schools: School[];
}) {
    const cache = useQueryClient();
    const q = useQuery({ queryKey: ['accounts'], queryFn: identity.accounts });
    const [search, setSearch] = useState(''), [status, setStatus] = useState(''), [binding, setBinding] = useState(''), [page, setPage] = useState(1);
    const [creating, setCreating] = useState(false), [selected, setSelected] = useState<string>(), [action, setAction] = useState<'details' | 'edit' | 'password' | 'binding'>('details');
    const refresh = () => cache.invalidateQueries({ queryKey: ['accounts'] });
    const records = q.data?.filter(a => Boolean(a.platform_admin) === administrators) ?? [];
    const rows = records.filter(a => `${value(a, 'username')} ${value(a, 'display_name')}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()) && (!status || String(a.active) === status) && (!binding || String(Boolean(a.wecom_userid)) === binding));
    const account = records.find(a => a.id === selected);
    const open = (a: Row, next: typeof action) => { setSelected(a.id); setAction(next); setCreating(false); };
    return <Card title={administrators ? '本地管理员列表' : '账号列表'} extra={!administrators && <Button type="primary" onClick={() => { setCreating(true); setSelected(undefined); }}>新建账号</Button>}>
        <p>{administrators ? '此处展示拥有平台管理员权限的账号，可管理所有学校。管理员身份不依赖昵称或学校成员角色。' : '此处管理全平台普通账号。创建账号不会自动分配学校，停用账号将影响其在所有学校的登录。'}</p>
        {administrators && <Alert type="info" title="管理员资料与身份受保护，此处支持查看详情和重置密码。"/>}
        {q.error && <Alert type="error" title={q.error.message} action={<Button onClick={() => void q.refetch()}>重试</Button>}/>}
        <Space wrap><Input aria-label="搜索账号" placeholder="搜索账号或昵称" value={search} onChange={e => { setSearch(e.target.value); setPage(1); }}/><Select aria-label="账号状态筛选" value={status} options={statusOptions} onChange={v => { setStatus(v); setPage(1); }}/><Select aria-label="企微绑定筛选" value={binding} options={[{ value: '', label: '全部绑定状态' }, { value: 'true', label: '已绑定企微' }, { value: 'false', label: '未绑定企微' }]} onChange={v => { setBinding(v); setPage(1); }}/></Space>
        <Table<Row> rowKey="id" loading={q.isPending} dataSource={rows} scroll={{ x: 800 }} pagination={{ current: page, pageSize: 10, onChange: setPage, showSizeChanger: false, showTotal: total => `共 ${total} 个账号` }} columns={[
            { title: '账号', render: (_, a) => value(a, 'username') },
            { title: '昵称', render: (_, a) => value(a, 'display_name') },
            { title: '身份', render: (_, a) => a.platform_admin ? '平台管理员' : '普通账号' },
            { title: '状态', render: (_, a) => <Tag>{a.active ? '启用' : '停用'}</Tag> },
            { title: '企业微信', render: (_, a) => a.wecom_userid ? `${value(a, 'wecom_name')} · ${value(a, 'wecom_userid')} · ${value(a, 'wecom_verification')}` : '未绑定' },
            { title: '操作', render: (_, a) => <Space wrap><Button onClick={() => open(a, 'details')}>详情</Button>{!administrators && a.id !== user.id && <><Button onClick={() => open(a, 'edit')}>编辑账号</Button><Button onClick={() => open(a, 'binding')}>绑定企微成员</Button></>}<Button onClick={() => open(a, 'password')}>重置密码</Button></Space> },
        ]}/>
        {creating && <Modal open title="新建账号" footer={null} onCancel={() => setCreating(false)}><Editor fields={[{ name: 'username', title: '账号' }, { name: 'display_name', title: '昵称' }, { name: 'password', title: '初始密码（至少12个字符）', type: 'password' }]} submit="创建账号" onSave={async v => { await identity.create({ username: v.username, display_name: v.display_name, password: v.password }); await refresh(); setCreating(false); }}/></Modal>}
        {account && <Modal open title={`${value(account, 'display_name')} · ${value(account, 'username')}`} footer={null} onCancel={() => setSelected(undefined)}>
            <p>账号 ID：{account.id}</p>
            {action === 'details' && (administrators ? <p>权限范围：全平台、所有学校，无需逐校授权。</p> : <AccountSchools account={account} schools={schools}/>)}
            {action === 'edit' && !administrators && <Editor key={`${account.id}:edit`} fields={[{ name: 'display_name', title: '昵称', value: value(account, 'display_name') }, { name: 'active', title: '账号状态（全平台）', value: String(account.active), options: statusOptions.slice(1) }]} onSave={async v => { await identity.update(account.id, { display_name: v.display_name, active: v.active === 'true' }); await refresh(); }}/>}
            {action === 'binding' && !administrators && <WecomBinding key={account.id} account={account} refresh={refresh}/>}
            {action === 'password' && <><p>重置后将退出该账号的所有登录会话。{account.id === user.id && '这是当前登录账号，保存后需要重新登录。'}</p><Editor key={`${account.id}:password`} fields={[{ name: 'password', title: '新密码（至少12个字符）', type: 'password' }]} submit="确认重置密码" onSave={async v => { await identity.reset(account.id, v.password); if (account.id === user.id) { await cache.cancelQueries(); cache.removeQueries({ predicate: query => query.queryKey[0] !== 'session' }); cache.setQueryData(['session'], null); } }}/></>}
        </Modal>}
    </Card>;
}

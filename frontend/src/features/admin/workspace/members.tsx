import { Field } from '../../../shared/field'
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Alert, Button, Input, Select, Space, Table } from 'antd';
import { identity, workspace, type Row, type School } from './api';
import { memberStatus, verificationName } from './directory';
import { schoolRoleLabel } from '../../../app/shell';

export function Members({ school, platformAdmin, refresh }: { school: School; platformAdmin: boolean; refresh: () => Promise<void> }) {
    const members = useQuery({ queryKey: ['business', school.id, 'members'], queryFn: ({ signal }) => workspace.list(school.id, 'members', '', signal) });
    const accounts = useQuery({ queryKey: ['accounts'], queryFn: identity.accounts, enabled: platformAdmin });
    const [search, setSearch] = useState(''), [page, setPage] = useState(1), [selected, setSelected] = useState<string[]>([]);
    const [role, setRole] = useState('operator'), [busy, setBusy] = useState(false), [error, setError] = useState(''), [done, setDone] = useState('');
    const grants = new Map(members.data?.items.map(m => [m.id, m]));
    const rows: Row[] = platformAdmin
        ? (accounts.data ?? []).filter(a => !a.platform_admin).map(a => ({ ...a, role: grants.get(a.id)?.role ?? 'unassigned' }))
        : members.data?.items ?? [];
    const filtered = rows.filter(a => `${a.username ?? ''} ${a.display_name ?? ''}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
    const loading = members.isPending || (platformAdmin && accounts.isPending);
    const loadError = members.error ?? (platformAdmin ? accounts.error : null);
    async function reload() {
        await Promise.all([members.refetch(), ...(platformAdmin ? [accounts.refetch()] : [])]);
    }
    async function save() {
        setBusy(true); setError(''); setDone('');
        const pending = [...selected];
        try {
            for (const id of selected) {
                await workspace.mutate(school.id, `members/${id}`, { role }, 'PUT');
                pending.splice(pending.indexOf(id), 1);
            }
            setDone('学校授权已保存');
        } catch (e) { setError(`部分操作未完成，已保留待处理账号：${(e as Error).message}`); }
        finally { setSelected(pending); setBusy(false); await refresh(); }
    }
    return <section>
        <h2>{school.name} · 学校成员</h2>
        <p>{platformAdmin ? '下表展示普通账号及其在本校的授权状态。新建账号显示为“未授权”，勾选后即可分配学校角色。平台管理员拥有全平台权限，无需逐校授权。' : '学校授权由平台管理员维护。'}</p>
        <Space wrap><Input aria-label="搜索学校成员账号" placeholder="搜索账号或昵称" value={search} onChange={e => { setSearch(e.target.value); setPage(1); }}/><Button disabled={busy} loading={members.isFetching || (platformAdmin && accounts.isFetching)} onClick={() => void reload()}>刷新账号与授权</Button></Space>
        {loadError && <Alert type="error" title={`账号或学校授权加载失败：${loadError.message}`}/>}
        {error && <Alert type="error" title={error}/>}
        {done && <Alert type="info" role="status"  title={<>{done}</>} />}
        <Table<Row> rowKey="id" loading={loading} dataSource={loadError ? [] : filtered} scroll={{ x: 600 }} locale={{ emptyText: loadError ? '加载失败，请刷新重试' : platformAdmin ? '没有匹配的普通账号，请先在账号管理中新建账号' : '本校暂无成员' }} pagination={{ current: page, pageSize: 10, showSizeChanger: false, onChange: setPage }} rowSelection={platformAdmin ? {
            selectedRowKeys: selected,
            onChange: keys => setSelected(keys.map(String)),
            preserveSelectedRowKeys: true,
            getCheckboxProps: () => ({ disabled: busy || loading || !!loadError }),
        } : undefined} columns={[
            { title: '账号', render: (_, a) => String(a.username ?? '') },
            { title: '昵称', render: (_, a) => String(a.display_name ?? '') },
            { title: '企业姓名 / 部门', render: (_, a) => `${a.wecom_name ?? '未绑定'} · ${Array.isArray(a.wecom_departments) ? a.wecom_departments.join(' / ') : ''}` },
            { title: '企业状态', render: (_, a) => `${memberStatus(typeof a.wecom_status === 'number' ? a.wecom_status : null)} · ${verificationName(String(a.wecom_verification ?? 'unverified'))}` },
            { title: '本校角色', render: (_, a) => a.role === 'unassigned' ? '未授权' : schoolRoleLabel(String(a.role)) },
            ...(platformAdmin ? [
                { title: '账号状态', render: (_: unknown, a: Row) => a.active ? '启用' : '停用' },
                { title: '企业微信', render: (_: unknown, a: Row) => a.wecom_userid ? String(a.wecom_userid) : '未绑定' },
            ] : []),
        ]}/>
        {platformAdmin && <div className="business-form"><p>已选择 {selected.length} 个账号。操作仅影响【{school.name}】的授权，不改变账号启停或其他学校角色。</p><Field label={<>所选账号在【{school.name}】的角色</>}><Select aria-label="学校角色" className="full-width" disabled={busy} value={role} onChange={value => setRole(value)}><Select.Option value="member">普通成员</Select.Option><Select.Option value="operator">运营</Select.Option><Select.Option value="admin">学校管理员</Select.Option><Select.Option value="revoked">撤销学校授权</Select.Option></Select></Field><Button type="primary" disabled={busy || loading || !!loadError || !selected.length} onClick={() => void save()}>保存所选成员授权</Button></div>}
    </section>;
}

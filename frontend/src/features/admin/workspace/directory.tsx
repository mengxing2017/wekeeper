import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Card, Input, Popconfirm, Select, Space, Table } from 'antd';
import { directory, type DirectoryMember, type DirectoryDepartment, type Row } from './api';

export const memberStatus = (status: number | null) => ({1:'已激活',2:'已禁用',4:'未激活',5:'退出企业'})[status ?? 0] ?? '状态待核验';
export const verificationName = (value: string) => ({verified:'已核验',unavailable:'不可见或不存在',unverified:'待核验'})[value] ?? '待核验';
function departmentPath(id: number, departments: DirectoryDepartment[], seen = new Set<number>()): string {
    if (seen.has(id)) return `部门 ${id}`;
    seen.add(id);
    const d = departments.find(d => d.id === id);
    if (!d) return `不可见部门 ${id}`;
    return d.parentid && d.parentid !== d.id ? `${departmentPath(d.parentid, departments, seen)} / ${d.name}` : d.name;
}
function DirectoryBrowser({ account, onSelect, busy = false }: { account?: string; onSelect?: (m: DirectoryMember) => void; busy?: boolean }) {
    const [page,setPage]=useState(1),[search,setSearch]=useState(''),[department,setDepartment]=useState<number>(),[status,setStatus]=useState<number>(),[binding,setBinding]=useState<'bound'|'unbound'>();
    const departments=useQuery({queryKey:['wecom-directory','departments'],queryFn:directory.departments});
    const members=useQuery({queryKey:['wecom-directory','members',page,search,department,status,binding],queryFn:()=>directory.members({page,search,department_id:department,status,binding})});
    return <section>
        <Space wrap><Input aria-label="搜索企业成员" placeholder="企业姓名或 UserID" value={search} onChange={e=>{setSearch(e.target.value);setPage(1);}}/>
            <Select aria-label="企业部门" placeholder="全部部门" allowClear style={{minWidth:200}} value={department} options={departments.data?.map(d=>({value:d.id,label:departmentPath(d.id,departments.data ?? [])}))} onChange={v=>{setDepartment(v);setPage(1);}}/>
            <Select aria-label="企业成员状态" placeholder="全部成员状态" allowClear style={{minWidth:160}} value={status} options={[1,2,4,5].map(value=>({value,label:memberStatus(value)}))} onChange={v=>{setStatus(v);setPage(1);}}/>
            <Select aria-label="企业成员绑定状态" placeholder="全部绑定状态" allowClear style={{minWidth:160}} value={binding} options={[{value:'bound',label:'已绑定'},{value:'unbound',label:'未绑定'}]} onChange={v=>{setBinding(v);setPage(1);}}/>
        </Space>
        {departments.error && <Alert type="error" title={`部门加载失败：${departments.error.message}`} action={<Button onClick={()=>void departments.refetch()}>重试部门</Button>}/>}
        {members.error && <Alert type="error" title={`成员加载失败：${members.error.message}`} action={<Button onClick={()=>void members.refetch()}>重试成员</Button>}/>}
        <Table<DirectoryMember> rowKey="id" loading={members.isFetching} dataSource={members.error ? [] : members.data?.items} pagination={false} scroll={{x:700}} locale={{emptyText:'没有匹配的已同步成员，请检查筛选、同步状态和应用可见范围'}} columns={[
            {title:'企业姓名',dataIndex:'name'}, {title:'UserID',dataIndex:'userid'},
            {title:'部门',render:(_,m)=>m.departments.map(d=>departmentPath(d.id,departments.data ?? m.departments)).join('；') || '部门不可见'},
            {title:'企业状态',render:(_,m)=>`${memberStatus(m.status)} · ${verificationName(m.verification)}${m.visibility==='not_seen'?' · 不在当前快照':''}`},
            {title:'本地账号',render:(_,m)=>m.bound_account_name ?? '未绑定'},
            {title:'核验时间',render:(_,m)=>m.checked_at ?? '尚未核验'},
            ...(onSelect ? [{title:'操作',render:(_:unknown,m:DirectoryMember)=><Button disabled={busy || m.status!==1 || m.verification!=='verified' || m.visibility!=='visible' || (!!m.bound_account_id && m.bound_account_id!==account)} onClick={()=>onSelect(m)}>选择成员</Button>}] : []),
        ]}/>
        <Space><Button disabled={page===1 || members.isFetching} onClick={()=>setPage(p=>p-1)}>上一页成员</Button><span>第 {page} 页</span><Button disabled={!members.data?.page?.has_more || members.isFetching} onClick={()=>setPage(p=>p+1)}>下一页成员</Button></Space>
    </section>;
}
export function Directory() {
    const cache=useQueryClient();
    const status=useQuery({queryKey:['wecom-directory','status'],queryFn:directory.status,refetchInterval:3000});
    const [busy,setBusy]=useState(false),[error,setError]=useState('');
    const run=status.data?.latest_run;
    async function sync(){setBusy(true);setError('');try{await directory.sync();await status.refetch();}catch(e){setError((e as Error).message);}finally{setBusy(false);}}
    async function rotate(){if(!status.data?.credential_generation)return;setBusy(true);setError('');try{await directory.rotate(status.data.credential_generation);await cache.invalidateQueries({queryKey:['wecom-directory']});}catch(e){setError((e as Error).message);}finally{setBusy(false);}}
    return <Card title="应用可见通讯录">
        <p>仅显示当前企业自建应用可见的部门和成员。同步不会创建本地账号或分配学校角色。</p>
        {status.isPending && <p role="status">正在读取同步状态…</p>}
        {status.error && <Alert type="error" title={status.error.message} action={<Button onClick={()=>void status.refetch()}>重试状态</Button>}/>}
        {status.data?.configuration_error && <Alert type="error" title={status.data.configuration_error}/>}
        <p>最近完整同步：{status.data?.last_success_at ?? '尚未同步'}。目录可能已变化，绑定和企业登录时会重新核验。</p>
        {run && <p role="status">最近任务：{({queued:'排队中',running:'同步中',succeeded:'同步成功',failed:'同步失败'})[run.state]} · {run.department_count} 个部门 / {run.member_count} 位成员 {run.error_code && `· ${run.error_code}`}</p>}
        {error && <Alert type="error" title={error}/>}
        <Space><Button onClick={()=>void sync()} disabled={busy || !status.data?.server_configured || !!status.data?.configuration_error || run?.state==='queued' || run?.state==='running'}>立即同步</Button><Button onClick={()=>void cache.invalidateQueries({queryKey:['wecom-directory']})}>刷新目录与状态</Button></Space>
        {status.data?.server_configured && status.data.credential_rotation_required && <p><Popconfirm title="验证并切换服务器当前凭据？" description="旧企业会话将退出，旧任务停止，成员待重新核验；账号绑定和学校授权保留。" onConfirm={()=>void rotate()} okText="验证并切换" cancelText="取消"><Button loading={busy} disabled={busy}>验证并切换服务端凭据</Button></Popconfirm></p>}
        <DirectoryBrowser key={status.data?.snapshot_id ?? 'empty'}/>
    </Card>;
}
export function WecomBinding({account,refresh}:{account:Row;refresh:()=>Promise<unknown>}){
    const cache=useQueryClient();
    const [selected,setSelected]=useState<DirectoryMember>(),[busy,setBusy]=useState(false),[error,setError]=useState(''),[done,setDone]=useState('');
    async function save(unbind=false){setBusy(true);setError('');setDone('');try{
        if(unbind) await directory.unbind(account.id,Number(account.wecom_binding_revision ?? 0));
        else if(selected) await directory.bind(account.id,selected.id,Number(account.wecom_binding_revision ?? 0));
        setSelected(undefined);setDone(unbind?'已解除企业微信绑定':'企业成员绑定已核验保存');
        await refresh();await cache.invalidateQueries({queryKey:['wecom-directory']});
    }catch(e){setError((e as Error).message);}finally{setBusy(false);}}
    return <section><h3>绑定企业微信成员</h3><p>当前绑定：{String(account.wecom_name ?? '')} {String(account.wecom_userid ?? '未绑定')}。绑定变更将退出此账号现有会话，不改变学校角色。</p>
        <a href="/admin?view=wecom-directory">查看同步状态或同步通讯录</a>
        {error && <Alert type="error" title={error}/>}{done && <p role="status">{done}</p>}
        <DirectoryBrowser account={account.id} onSelect={m=>{setSelected(m);setDone('');}} busy={busy}/>
        {selected && <p>待绑定：{selected.name} · {selected.userid} · {selected.departments.map(d=>d.name).join(' / ')}</p>}
        <Space><Button type="primary" disabled={busy || !selected || !account.active} onClick={()=>void save()}>核验并保存绑定</Button><Button disabled={busy || !account.wecom_member_id} onClick={()=>void save(true)}>解除绑定</Button><Button disabled={busy} onClick={()=>void refresh()}>刷新账号绑定版本</Button></Space>
    </section>;
}

import { formatDate, formatUnixSeconds } from '../../../shared/format-date'
import { PaginationControls as Pager } from '../../../shared/pagination-controls'
import { useEffect, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Card, Drawer, Input, Select, Space, Table, Tabs } from 'antd';
import { assets, directory, type AssetList, type AssetFilter, type GroupCandidate, type Row } from './api';
import { verificationName } from './directory';
const formatDateValue = (value: unknown) => formatDate(value);
const show = (v: unknown) => v === null || v === undefined ? '—' : typeof v === 'object' ? JSON.stringify(v) : String(v);
const fieldLabels:Record<string,string>={name:'名称',userid:'UserID',chat_id:'客户群标识',owner:'群主 UserID',member_name:'企业成员',member_count:'成员数量',checked_at:'核验时间',verification:'核验状态',source:'资料来源',created_at:'创建时间',started_at:'开始时间',finished_at:'结束时间',dataset:'数据集',state:'任务状态',stage:'任务阶段',processed:'已处理项目',total:'枚举成员 / 群主数量',pages:'已读取页数',error_code:'错误码',scope_count:'范围成员数量',scope_changed:'范围是否缩小',complete:'采集是否完整'};
const stateName = (v: unknown) => ({queued:'排队中',running:'同步中',succeeded:'同步成功',failed:'同步失败'})[String(v)] ?? show(v);
export function Assets() {
    const cache=useQueryClient(), [kind,setKind]=useState<AssetList>('groups'),[filter,setFilter]=useState<AssetFilter>({page:1}),[error,setError]=useState(''),[busy,setBusy]=useState(false),[relationship,setRelationship]=useState<'owner'|'participant'>('owner'),[selected,setSelected]=useState<{kind:'groups'|'sync-runs';id:string}>();
    const status=useQuery({queryKey:['wecom-assets','status'],queryFn:assets.status,refetchInterval:3000});
    const list=useQuery({queryKey:['wecom-assets','list',kind,filter],queryFn:()=>assets.list(kind,filter)});
    const [memberSearch,setMemberSearch]=useState('');
    const members=useQuery({queryKey:['wecom-assets','member-options',memberSearch],queryFn:()=>directory.members({search:memberSearch})});
    const published=status.data?.datasets.map(d=>d.current_run_id).join(',');const previous=useRef<string|undefined>(undefined);
    useEffect(()=>{if(published!==previous.current){previous.current=published;void cache.invalidateQueries({queryKey:['wecom-assets','list']});void cache.invalidateQueries({queryKey:['wecom-assets','detail']});void cache.invalidateQueries({queryKey:['wecom-group-candidates']});}},[published,cache]);
    const running=status.data?.datasets.some(d=>d.latest_run?.state==='running'||d.latest_run?.state==='queued');
    useEffect(()=>{if(running){const timer=setInterval(()=>void cache.invalidateQueries({queryKey:['wecom-assets','list','sync-runs']}),3000);return ()=>clearInterval(timer);}},[running,cache]);
    async function sync(){setBusy(true);setError('');try{await assets.sync('groups');await status.refetch();await cache.invalidateQueries({queryKey:['wecom-assets','list','sync-runs']});}catch(e){setError((e as Error).message);}finally{setBusy(false);}}
    function update(v:Partial<AssetFilter>){setFilter(f=>({...f,...v,page:1}));}
    return <Card title="客户群管理">
        <p>同步当前应用可见的客户群与群成员，按运营成员查看群主群和参与群。同步不会发送消息或分配学校权限。</p>
        {(error||status.error||status.data?.configuration_error)&&<Alert type="error" title={error||status.error?.message||status.data?.configuration_error}/>}
        <Button loading={busy} disabled={!status.data?.server_configured||!!status.data?.configuration_error||running} onClick={()=>void sync()}>同步客户群</Button>
        {status.data?.datasets.map(d=><div key={d.dataset}><p><strong>客户群与成员</strong> · 最近完整成功：{formatDate(d.last_success_at, '尚未同步')} · {stateName(d.latest_run?.state)} · 已处理 {d.latest_run?.processed??0} · 页数 {d.latest_run?.pages??0}</p>{d.latest_run?.error_code&&<Alert type="error" title={`同步失败：${d.latest_run.error_code}；保留上次成功快照`}/>} {d.scope_changed&&<Alert type="warning" title="应用可见范围缩小：缺失数据已标记待核验，统计仅包含当前可见资产"/>}{d.freshness==='unverified'&&<Alert type="info" title="当前凭据代次尚无完整快照"/>}{d.freshness==='stale'&&<Alert type="warning" title="快照已超过 24 小时，请重新同步；过期数据不进入候选和当前统计"/>}</div>)}
        <Tabs activeKey={kind} onChange={k=>{setKind(k as AssetList);setFilter({page:1});}} items={[{key:'groups',label:'客户群'},{key:'member-statistics',label:'运营成员统计'},{key:'sync-runs',label:'同步任务'}]}/>
        <Space wrap>{kind!=='sync-runs'&&<Input aria-label="搜索客户群" placeholder="名称或官方标识" value={filter.search??''} onChange={e=>update({search:e.target.value})}/>}
            {['groups','member-statistics'].includes(kind)&&<Select aria-label="筛选运营成员" placeholder="全部运营成员" allowClear showSearch filterOption={false} onSearch={setMemberSearch} style={{minWidth:220}} value={filter.member_id??filter.owner_member_id??filter.participant_member_id} options={members.data?.items.map(m=>({value:m.id,label:`${m.name} (${m.userid})`}))} onChange={id=>update(kind==='groups'?{owner_member_id:relationship==='participant'?undefined:id,participant_member_id:relationship==='participant'?id:undefined}:{member_id:id})}/>}
            {kind==='groups'&&<Select aria-label="成员群关系" value={relationship} options={[{value:'owner',label:'担任群主'},{value:'participant',label:'参与客户群'}]} onChange={v=>{setRelationship(v);update({owner_member_id:v==='owner'?(filter.participant_member_id??filter.owner_member_id):undefined,participant_member_id:v==='participant'?(filter.owner_member_id??filter.participant_member_id):undefined});}}/>}
            {kind==='groups'&&<Select aria-label="学校关联状态" placeholder="全部关联状态" allowClear style={{minWidth:160}} value={filter.associated===undefined?undefined:String(filter.associated)} options={[{value:'true',label:'已关联学校'},{value:'false',label:'未关联学校'}]} onChange={value=>update({associated:value===undefined?undefined:value==='true'})}/>}
            {kind==='groups'&&<Select aria-label="资产核验状态" placeholder="全部核验状态" allowClear style={{minWidth:160}} value={filter.verification} options={['verified','unverified','unavailable'].map(v=>({value:v,label:verificationName(v)}))} onChange={v=>update({verification:v})}/>}
            <Button onClick={()=>void list.refetch()}>刷新目录</Button>
        </Space>
        {list.error&&<Alert type="error" title={list.error.message}/>}
        <Table<Row> rowKey="id" loading={list.isFetching} dataSource={list.data?.items} pagination={false} scroll={{x:800}} columns={kind==='sync-runs'?[
            {title:'状态',render:(_,r)=>stateName(r.state)},{title:'阶段',dataIndex:'stage'},{title:'已处理 / 预计范围',render:(_,r)=>`${show(r.processed)} / ${show(r.total)}`},{title:'页数',dataIndex:'pages'},{title:'错误码',dataIndex:'error_code'},{title:'创建时间',dataIndex:'created_at',render:formatDateValue},{title:'详情',render:(_,r)=><Button size="small" type="link" onClick={()=>setSelected({kind:'sync-runs',id:r.id})}>任务详情</Button>},
        ]:kind==='member-statistics'?[
            {title:'企业成员',dataIndex:'name'},{title:'UserID',dataIndex:'userid'},{title:'群主群',dataIndex:'owned_group_count'},{title:'参与群',dataIndex:'participating_group_count'},
        ]:[
            {title:'名称 / 成员',render:(_,r)=>show(r.name??r.member_name??r.userid)},{title:'官方标识',render:(_,r)=>show(r.chat_id??r.userid)},
            {title:'群主',render:(_,r)=>show(r.member_name??r.owner)},
            ...(kind==='groups'?[{title:'人数',dataIndex:'member_count'},{title:'学校关联',render:(_:unknown,r:Row)=>r.school_id?'已关联':'未关联'}]:[]),
            {title:'核验',render:(_,r)=>`${verificationName(String(r.verification))}${r.visibility==='not_seen'?' · 本轮未见':''}`},{title:'核验时间',dataIndex:'checked_at',render:formatDateValue},
            ...(kind==='groups'?[{title:'详情',render:(_:unknown,r:Row)=><Button size="small" type="link" onClick={()=>setSelected({kind:kind as 'groups',id:r.id})}>查看关系</Button>}]:[]),
        ]}/>
        <Pager page={filter.page??1} more={list.data?.page?.has_more} busy={list.isFetching} change={page=>setFilter(f=>({...f,page}))}/>
        {selected&&<AssetDetail key={`${selected.kind}:${selected.id}`} {...selected} close={()=>setSelected(undefined)}/>}
    </Card>;
}
function AssetDetail({kind,id,close}:{kind:'groups'|'sync-runs';id:string;close:()=>void}){
    const [page,setPage]=useState(1),[reason,setReason]=useState(''),[scopeError,setScopeError]=useState(''),[scopeBusy,setScopeBusy]=useState(false);const detail=useQuery({queryKey:['wecom-assets','detail',kind,id],queryFn:()=>assets.detail(kind,id),refetchInterval:kind==='sync-runs'?3000:false});
    const relations=useQuery({queryKey:['wecom-assets','detail',kind,id,page],queryFn:()=>assets.relations(kind as 'groups',id,page),enabled:kind!=='sync-runs'});
    return <Drawer open onClose={close} title={kind==='sync-runs'?'同步任务详情':'资产关系'} size="large">
        {(detail.error||relations.error)&&<Alert type="error" title={detail.error?.message??relations.error?.message}/>}
        {detail.data&&<dl>{Object.entries(detail.data).filter(([k])=>fieldLabels[k]).map(([k,v])=><div key={k}><dt>{fieldLabels[k]}</dt><dd>{k.endsWith('_at')?formatDate(v):k==='state'?stateName(v):k==='verification'?verificationName(String(v)):show(v)}</dd></div>)}</dl>}
        {kind==='sync-runs'&&detail.data?.failed_owner&&<Space orientation="vertical"><Alert type="warning" title={`历史群主 ${show(detail.data.failed_owner)} 无法枚举。先核对权限；确认已不属于可见范围后，可保留其历史数据并从后续历史补扫中排除。在职或重新可见成员仍会被扫描。`}/><Input aria-label="范围变更依据" placeholder="填写核对依据（至少 5 个字）" value={reason} onChange={e=>setReason(e.target.value)}/><Button disabled={reason.trim().length<5||scopeBusy} onClick={async()=>{setScopeBusy(true);try{await assets.confirmOwnerScope({run_id:id,owner_userid:String(detail.data?.failed_owner),expected_generation:Number(detail.data?.generation),reason});setReason('');await detail.refetch();}catch(e){setScopeError((e as Error).message);}finally{setScopeBusy(false);}}}>确认历史群主范围变化</Button>{scopeError&&<Alert type="error" title={scopeError}/>}</Space>}
        {kind!=='sync-runs'&&<><Table rowKey="id" loading={relations.isFetching} dataSource={relations.data?.items} pagination={false} columns={[{title:'成员',render:(_,r)=>show(r.member_name??r.userid)},{title:'身份',render:(_,r)=>r.member_type===1?'企业成员':'外部联系人'},{title:'资料',render:(_,r)=>r.member_id?'已解析企业成员':'仅保留群成员标识'},{title:'入群时间',render:(_,r)=>formatUnixSeconds(r.join_time)},{title:'核验状态',render:(_,r)=>verificationName(r.verification)}]}/><Pager page={page} more={relations.data?.page?.has_more} busy={relations.isFetching} change={setPage}/></>}
    </Drawer>;
}
export function GroupPicker({school,onSaved}:{school:string;onSaved:()=>Promise<void>}){
    const [page,setPage]=useState(1),[search,setSearch]=useState(''),[selected,setSelected]=useState<GroupCandidate>(),[name,setName]=useState(''),[error,setError]=useState(''),[busy,setBusy]=useState(false);
    const q=useQuery({queryKey:['wecom-group-candidates',school,page,search],queryFn:()=>assets.candidates(school,{page,search}),retry:false});
    async function save(){if(!selected)return;setBusy(true);setError('');try{await assets.bind(school,{directory_group_id:selected.id,expected_generation:selected.credential_generation,expected_checked_at:selected.checked_at,...(name.trim()?{name:name.trim()}:{})});setSelected(undefined);setName('');await q.refetch();await onSaved();}catch(e){setError((e as Error).message);setSelected(undefined);await q.refetch();}finally{setBusy(false);}}
    return <Card size="small" title="从同步目录选择客户群"><p>学校管理员仅可选择自己担任群主的群。未绑定企业成员时，请联系平台管理员配置。</p>
        <Input aria-label="搜索候选客户群" value={search} placeholder="群名或 chat_id" onChange={e=>{setSearch(e.target.value);setPage(1);setSelected(undefined);}}/>
        {(error||q.error)&&<Alert type="error" title={error||q.error?.message} action={<Button onClick={()=>void q.refetch()}>刷新候选</Button>}/>}
        <Table<GroupCandidate> rowKey="id" loading={q.isFetching} dataSource={q.error?[]:q.data?.items} pagination={false} locale={{emptyText:'暂无可选群：请检查同步状态、绑定身份及群主范围；其他学校已关联的群不会显示'}} columns={[{title:'官方群名',dataIndex:'name'},{title:'群主',render:(_,g)=>g.owner_name??g.owner},{title:'人数',dataIndex:'member_count'},{title:'操作',render:(_,g)=><Button size="small" type="link" disabled={busy||g.already_associated} onClick={()=>{setSelected(g);setName(g.name??'');}}>{g.already_associated?'已关联本校':'选择群'}</Button>}]}/>
        <Pager page={page} more={q.data?.page?.has_more} busy={q.isFetching} change={setPage}/>
        {selected&&<Space orientation="vertical"><p>已选 {selected.name??selected.chat_id} · {selected.corp_id} · {selected.chat_id}</p><Input aria-label="学校群显示名" value={name} onChange={e=>setName(e.target.value)} maxLength={200}/><Button loading={busy} onClick={()=>void save()}>关联到当前学校</Button></Space>}
    </Card>;
}

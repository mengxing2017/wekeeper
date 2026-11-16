import { PaginationControls } from '../../../shared/pagination-controls'
import { formatDate } from '../../../shared/format-date'
import { TableText } from '../../../shared/table-text'
import {businessMessages} from '../source-access/api'
import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Drawer,Input,Popconfirm,Select,Space,Table,Typography} from 'antd'
import {scopedMaterials,adPlacements,advertisingScopes,type ScopedMaterial,type AdContent,type PlacementScope,type AdPlacement} from './api'
const blank:AdContent={name:'',body:'',link:null}
export function Advertising(){
 const [page,setPage]=useState(1),[bindingPage,setBindingPage]=useState(1),[selected,setSelected]=useState<ScopedMaterial|null>(),[binding,setBinding]=useState(false),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const materials=useQuery({queryKey:['scoped-ad-materials',page],queryFn:()=>scopedMaterials.list(page)})
 const placements=useQuery({queryKey:['ad-placements',bindingPage],queryFn:()=>adPlacements.list(bindingPage)})
 const scopes=useQuery({queryKey:['advertising-scopes'],queryFn:advertisingScopes})
 const names=new Map(scopes.data?.platforms.map(p=>[p.id,p.name]))
 const unitNames=new Map(scopes.data?.units.map(u=>[u.id,u.name]))
 const scopeName=(scope:PlacementScope)=>scope.kind==='global'?'全局':scope.kind==='platform'?`平台：${names.get(scope.platform_id)??scope.platform_id}`:`业务单位：${unitNames.get(scope.unit_id)??scope.unit_id}`
 const toggle=async(row:AdPlacement)=>{setBusy(true);setError('');try{await adPlacements.setState(row.id,{expected_revision:row.revision,enabled:!row.enabled});await placements.refetch()}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 return <Space orientation="vertical" style={{width:'100%'}}>
  <Alert type="info" title="广告按单位、平台、全局的优先级匹配，在同一范围内轮询广告。每条消息最多插入一个广告；不可用时直接发送原文。广告轮询进度按业务单位分别计数。"/>
  {(error||materials.error||placements.error||scopes.error)&&<Alert type="error" title={error||materials.error?.message||placements.error?.message||scopes.error?.message}/>}
  <Space wrap><Button type="primary" onClick={()=>setSelected(null)}>新增</Button><Button onClick={()=>setBinding(true)}>绑定投放范围</Button><Button onClick={()=>{void materials.refetch();void placements.refetch()}}>刷新</Button></Space>
  <Table tableLayout="fixed" scroll={{x:620}} rowKey="id" loading={materials.isFetching} dataSource={materials.data?.items??[]} pagination={false} columns={[{title:'名称', width:200,render:(_,r)=><TableText value={r.content.name} limit={24}/>},{title:'正文',render:(_,r)=><TableText value={r.content.body} limit={60}/>},{title:'状态', width:100,render:(_,r)=>r.archived?'已归档':'可投放'},{title:'操作', width:88,render:(_,r)=><Button type="link" disabled={r.archived} onClick={()=>setSelected(r)}>编辑</Button>}]}/>
  <PaginationControls page={page} more={materials.data?.has_more} busy={materials.isFetching} change={setPage} subject="广告"/>
  <Typography.Title level={5}>投放绑定</Typography.Title>
  <Table tableLayout="fixed" scroll={{x:960}} rowKey="id" loading={placements.isFetching} dataSource={placements.data?.items??[]} pagination={false} columns={[{title:'广告', width:200,render:(_,r)=><TableText value={materials.data?.items.find(m=>m.id===r.material_id)?.content.name??r.material_id}/>},{title:'投放范围',render:(_,r)=><TableText value={scopeName(r.scope)}/>},{title:'生效时间', width:180,dataIndex:'valid_from',render:value=>formatDate(value)},{title:'到期时间', width:180,render:(_,r)=>r.valid_until?formatDate(r.valid_until):'长期有效'},{title:'状态', width:100,render:(_,r)=>r.enabled?'已启用':'已停用'},{title:'操作', width:88,render:(_,r)=><Button type="link" loading={busy} onClick={()=>void toggle(r)}>{r.enabled?'停用':'启用'}</Button>}]}/>
  <PaginationControls page={bindingPage} more={placements.data?.has_more} busy={placements.isFetching} change={setBindingPage} subject="投放绑定"/>
  <Drawer open={selected!==undefined} title={selected?'编辑广告':'新增广告'} destroyOnHidden onClose={()=>setSelected(undefined)}>{selected!==undefined&&<MaterialEditor key={selected?.id??'new'} initial={selected} onSaved={async()=>{setSelected(undefined);await materials.refetch()}}/>}</Drawer>
  <Drawer open={binding} title="绑定投放范围" destroyOnHidden onClose={()=>setBinding(false)}>{binding&&<PlacementEditor onSaved={async()=>{setBinding(false);await placements.refetch()}}/>}</Drawer>
 </Space>
}
function MaterialEditor({initial,onSaved}:{initial:ScopedMaterial|null;onSaved:()=>Promise<void>}){
 const [content,setContent]=useState(initial?.content??blank),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const save=async()=>{setError('');if(Array.from(content.body).length>140){setError('单个广告正文最多 140 字');return}setBusy(true);try{if(initial)await scopedMaterials.publish(initial.id,{expected_revision:initial.revision,content});else await scopedMaterials.create({content});await onSaved()}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 const archive=async()=>{setError('');if(!initial)return;setBusy(true);try{await scopedMaterials.archive(initial.id,{expected_revision:initial.revision});await onSaved()}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 return <Space orientation="vertical" style={{width:'100%'}}>{error&&<Alert type="error" title={error}/>}<Input aria-label="广告名称" placeholder="名称" value={content.name} onChange={e=>setContent({...content,name:e.target.value})}/><Input.TextArea aria-label="广告正文" placeholder="正文，最多 140 字" count={{show:true,max:140,strategy:value=>Array.from(value).length}} value={content.body} onChange={e=>setContent({...content,body:e.target.value})}/><Input aria-label="广告链接" placeholder="链接（可选）" value={content.link??''} onChange={e=>setContent({...content,link:e.target.value||null})}/><Button type="primary" loading={busy} onClick={()=>void save()}>保存</Button>{initial&&<Popconfirm title="归档后不再用于新消息，历史正文与结果继续保留" onConfirm={()=>void archive()}><Button danger loading={busy}>归档</Button></Popconfirm>}</Space>
}
function PlacementEditor({onSaved}:{onSaved:()=>Promise<void>}){
 const [material,setMaterial]=useState<string>(),[kind,setKind]=useState<PlacementScope['kind']>('unit'),[platform,setPlatform]=useState<string>(),[unit,setUnit]=useState<string>(),[page,setPage]=useState(1),[from,setFrom]=useState(new Date().toISOString()),[until,setUntil]=useState(''),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const materials=useQuery({queryKey:['scoped-ad-materials',page],queryFn:()=>scopedMaterials.list(page)})
 const scopes=useQuery({queryKey:['advertising-scopes'],queryFn:advertisingScopes})
 const save=async()=>{setError('');if(!material||(kind!=='global'&&!platform)||(kind==='unit'&&!unit)){setError('请选择广告及投放范围');return}if(!Number.isFinite(Date.parse(from))||(until&&!Number.isFinite(Date.parse(until)))){setError('请填写有效的生效和到期时间');return}setBusy(true);try{const scope:PlacementScope=kind==='global'?{kind}:kind==='platform'?{kind,platform_id:platform!}:{kind,platform_id:platform!,unit_id:unit!};await adPlacements.create({material_id:material,scope,enabled:true,valid_from:new Date(from).toISOString(),valid_until:until?new Date(until).toISOString():null});await onSaved()}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 return <Space orientation="vertical" style={{width:'100%'}}>{(error||materials.error||scopes.error)&&<Alert type="error" title={error||materials.error?.message||scopes.error?.message}/>}
  <Select aria-label="选择广告" placeholder="选择广告" value={material} onChange={setMaterial} options={materials.data?.items.filter(m=>!m.archived).map(m=>({value:m.id,label:m.content.name}))}/>
  <PaginationControls page={page} more={materials.data?.has_more} busy={materials.isFetching} change={setPage} subject="可选广告"/>
  <Select aria-label="投放范围" value={kind} onChange={v=>{setKind(v);setPlatform(undefined);setUnit(undefined)}} options={[{value:'unit',label:'业务单位'},{value:'platform',label:'平台'},{value:'global',label:'全局'}]}/>
  {kind!=='global'&&<Select aria-label="选择平台" placeholder="选择平台" value={platform} onChange={v=>{setPlatform(v);setUnit(undefined)}} options={scopes.data?.platforms.map(p=>({value:p.id,label:p.name}))}/>}
  {kind==='unit'&&<Select aria-label="选择业务单位" placeholder="选择业务单位" value={unit} onChange={setUnit} options={scopes.data?.units.filter(u=>u.platform_id===platform).map(u=>({value:u.id,label:u.name}))}/>}
  <Input aria-label="生效时间" placeholder="生效时间（含时区）" value={from} onChange={e=>setFrom(e.target.value)}/><Input aria-label="到期时间" placeholder="到期时间（可选，含时区）" value={until} onChange={e=>setUntil(e.target.value)}/><Button type="primary" loading={busy} onClick={()=>void save()}>保存</Button>
 </Space>
}
export function RouteAdvertising({route}:{route:string}){
 return <Alert type="info" title="消息按所属业务单位自动匹配并轮询广告，无需单独配置路由广告池。" action={<Button href={`/admin?view=advertisements&route=${encodeURIComponent(route)}`}>管理广告投放</Button>}/>
}
export function AdCompositions({route,receipt}:{route:string;receipt:string}){
 const q=useQuery({queryKey:['message-advertisement',route,receipt],queryFn:()=>businessMessages.list(route,receipt)})
 const message=q.data?.items[0]
 return <Space orientation="vertical"><Typography.Title level={5}>广告决定与发送正文</Typography.Title>{q.error&&<Alert type="error" title={q.error.message}/>}<p>{message?.decision?(message.decision.selected_ad?'已插入一个广告':'未插入广告'):(message?.state==='historical'?'历史消息保留原发送正文':(q.isFetching?'正在读取广告决定':'未形成广告决定'))}</p><Typography.Paragraph style={{whiteSpace:'pre-wrap'}}>{message?.content}</Typography.Paragraph></Space>
}

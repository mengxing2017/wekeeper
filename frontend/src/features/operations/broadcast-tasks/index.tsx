import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Checkbox, Input, Select, Space, Tabs, Tag } from 'antd'
import { Field } from '../../../shared/field'
import { PaginationControls } from '../../../shared/pagination-controls'
import { useDialogs } from '../../../shared/use-dialogs'
import { formatDate } from '../../../shared/format-date'
import { broadcast, workspace, type BroadcastTask, type BroadcastTarget, type BroadcastDraftInput, type BroadcastAction, type Post, type Row, type Draft, type Attempt } from './api'

const creation: Record<string, string> = {queued:'待提交',submitting:'创建中',created:'已创建，等待成员执行',rejected:'创建被拒绝',unknown:'创建结果不确定',cancelled:'已取消'}
export function targetLabel(t: BroadcastTarget) {
  if (t.official_status === 1) return '平台报告已发送（非已读）'
  if (t.rejected) return '平台拒绝此目标'
  if (t.official_status === 2) return '平台报告非好友失败（需核对）'
  if (t.official_status === 3) return '平台报告群发接收额度限制'
  if (t.official_status !== null && t.official_status !== 0) return `待核验状态：${t.official_status}`
  if (t.stop_state === 'stopped') return '已停止后续发送；未取得本群发送证据'
  return creation[t.creation_state] ?? '未取得结果'
}
const taskLabel = (t: BroadcastTask) => t.state === 'draft' ? '草稿' : t.state === 'abandoned' ? '已放弃' : t.uncertain ? '结果待核对' : t.sent === t.total && t.total > 0 ? '全部已发送' : t.stopped ? '已请求停止' : t.rejected ? '有拒绝结果' : t.sent ? '部分已发送' : '待创建 / 待成员执行'
export default function BroadcastTasks({school, admin, platformAdmin = false}: {school: string; admin: boolean; platformAdmin?: boolean}) {
  const [tab,setTab] = useState('tasks'), [selected,setSelected] = useState(''), [page,setPage] = useState(1), [status,setStatus] = useState('')
  const [editing,setEditing] = useState<{id?: string; input?: BroadcastDraftInput} | null>(null)
  const query = useQuery({queryKey:['business',school,'broadcast',page,status],queryFn:()=>broadcast.list(school,page,status)})
  const capability = useQuery({queryKey:['business',school,'broadcast-capability'],queryFn:()=>broadcast.capability(school)})
  const cache=useQueryClient()
  const refresh=async()=>{await cache.invalidateQueries({queryKey:['business',school]})}
  return <section>
    <p>统一创建客户群任务，成员仍需在企业微信确认发送。创建成功不等于消息已发送。</p>
    {capability.data && !capability.data.broadcast_available && <Alert type="warning" title={capability.data.reason}/>}
    {capability.error && <Alert type="error" title={capability.error.message}/>}
    <Tabs activeKey={tab} onChange={setTab} items={[{key:'tasks',label:'群发任务'},{key:'history',label:'历史人工分享'}]}/>
    {tab==='history' ? <History school={school} admin={admin} migrate={input=>{setEditing({input});setSelected('');setTab('tasks')}}/> : <>
      <Space><Button onClick={()=>{setEditing({});setSelected('')}}>新建群发任务</Button><Select aria-label="任务状态" value={status} onChange={v=>{setStatus(v);setPage(1)}} options={[{value:'',label:'全部任务'},{value:'draft',label:'草稿'},{value:'confirmed',label:'已确认'},{value:'pending',label:'待执行'},{value:'issues',label:'异常'},{value:'abandoned',label:'已放弃'}]}/><Button onClick={()=>void refresh()}>刷新列表</Button></Space>
      {editing && <DraftEditor school={school} initial={editing.input} id={editing.id} saved={async t=>{setEditing(null);setSelected(t.id);await refresh()}} cancel={()=>setEditing(null)}/>}
      {selected && <Detail key={selected} school={school} id={selected} admin={admin} platformAdmin={platformAdmin} enabled={capability.data?.broadcast_available ?? false} managementAvailable={capability.data?.management_available ?? false} refresh={refresh} edit={(id,input)=>{setEditing({id,input});setSelected('')}}/>}
      {query.error && <Alert type="error" title={query.error.message}/>}
      {query.isPending && <p role="status">正在加载任务…</p>}
      {query.data?.items.map(t=><Card key={t.id} title={<><Tag>{taskLabel(t)}</Tag>{formatDate(t.created_at)}</>}><p>{t.content.slice(0,100)}</p><p>{t.total} 群 · {t.batch_count} 批次 · 已发送 {t.sent} · 拒绝 {t.rejected} · 不确定批次 {t.uncertain}</p><Button onClick={()=>{setSelected(t.id);setEditing(null)}}>查看任务 {t.id.slice(-6)}</Button></Card>)}
      {query.isSuccess && !query.data.items.length && <p>暂无群发任务</p>}
      <PaginationControls page={page} more={query.data?.page?.has_more} busy={query.isFetching} change={setPage}/>
    </>}
  </section>
}
function DraftEditor({school,initial,id,saved,cancel}:{school:string;initial?:BroadcastDraftInput;id?:string;saved:(task:BroadcastTask)=>Promise<void>;cancel:()=>void}) {
  const [chosen,setChosen]=useState(initial?.post_ids??[]), [groups,setGroups]=useState(initial?.group_ids??[]), [version,setVersion]=useState(initial?.template_version_id??'')
  const [search,setSearch]=useState(''),[page,setPage]=useState(1),[reason,setReason]=useState(initial?.reason??'')
  const [error,setError]=useState(''),[busy,setBusy]=useState(false);const lock=useRef(false)
  const posts=useQuery({queryKey:['business',school,'broadcast-posts',search,page],queryFn:()=>workspace.list<Post>(school,'posts',`?status=approved&page[number]=${page}&page[size]=30&search=${encodeURIComponent(search)}`)})
  const targets=useQuery({queryKey:['business',school,'groups'],queryFn:()=>workspace.list(school,'groups')})
  const templates=useQuery({queryKey:['business',school,'templates'],queryFn:()=>workspace.list(school,'templates')})
  const versions=templates.data?.items.flatMap(t=>Array.isArray(t.versions)?(t.versions as Row[]).filter(v=>v.published).map(v=>({value:v.id,label:`${String(t.name)} · ${v.id.slice(-6)}`})):[])??[]
  async function save() {
    if(lock.current)return;lock.current=true;setBusy(true);setError('')
    try {await saved(await broadcast.save(school,{...initial,template_version_id:version,post_ids:chosen,group_ids:groups,reason},id))}catch(e){setError((e as Error).message)}finally{lock.current=false;setBusy(false)}
  }
  return <Card title={id?'修改任务草稿':'创建任务草稿'}>
    <p>选择 1–20 条已审核内容和本校客户群。保存仅生成预览，不创建企业微信任务。正文上限 4000 UTF-8 字节。</p>
    {[posts,targets,templates].map((q,i)=>q.error&&<Alert key={i} type="error" title={q.error.message}/>)}
    <Field label="搜索已审核内容"><Input value={search} onChange={e=>{setSearch(e.target.value);setPage(1)}}/></Field>
    <p>已选 {chosen.length} 条<Button onClick={()=>setChosen([])}>清空内容</Button></p>
    {posts.data?.items.filter(p=>p.visibility==='visible').map(p=><Checkbox className="check-row" key={p.id} checked={chosen.includes(p.id)} onChange={e=>setChosen(e.target.checked?[...chosen,p.id]:chosen.filter(id=>id!==p.id))}>{p.title}</Checkbox>)}
    <PaginationControls page={page} more={posts.data?.page?.has_more} busy={posts.isFetching} change={setPage} subject="内容"/>
    <Field label="已发布模板"><Select aria-label="已发布模板" className="full-width" value={version||undefined} options={versions} onChange={setVersion}/></Field>
    <Field label="目标客户群"><Select aria-label="目标客户群" className="full-width" mode="multiple" value={groups} options={targets.data?.items.filter(g=>g.active).map(g=>({value:g.id,label:String(g.name)}))} onChange={setGroups}/></Field>
    <p>保存时核验目录新鲜度和群主学校授权；确认前再次实时核验。失效群会阻止保存，不会静默跳过。</p>
    {(initial?.source_task_id||initial?.source_draft_id)&&<Field label="再次发送 / 迁移原因"><Input.TextArea value={reason} maxLength={1000} onChange={e=>setReason(e.target.value)}/></Field>}
    {error&&<Alert type="error" title={error}/>}
    <Space><Button type="primary" loading={busy} disabled={!chosen.length||!groups.length||!version} onClick={()=>void save()}>保存并预览</Button><Button disabled={busy} onClick={cancel}>关闭</Button></Space>
  </Card>
}
async function allTargets(s:string,id:string) {const rows:BroadcastTarget[]=[];for(let page=1;page<=100;page++){const result=await broadcast.targets(s,id,page);rows.push(...result.items);if(!result.page?.has_more)return rows}throw new Error('目标数量超过支持范围')}
function Detail({school,id,admin,platformAdmin,enabled,managementAvailable,refresh,edit}:{school:string;id:string;admin:boolean;platformAdmin:boolean;enabled:boolean;managementAvailable:boolean;refresh:()=>Promise<void>;edit:(id:string|undefined,input:BroadcastDraftInput)=>void}) {
  const [page,setPage]=useState(1),[eventPage,setEventPage]=useState(1),[error,setError]=useState(''),[busy,setBusy]=useState(false)
  const [batch,setBatch]=useState(''),[decision,setDecision]=useState<BroadcastAction['decision']>('not_created'),[reason,setReason]=useState(''),[msgid,setMsgid]=useState('')
  const [candidates,setCandidates]=useState<Row[]>([])
  const keys=useRef(new Map<string,string>()),lock=useRef(false),dialogs=useDialogs()
  const task=useQuery({queryKey:['business',school,'broadcast-task',id],queryFn:()=>broadcast.task(school,id),refetchInterval:15000})
  const targets=useQuery({queryKey:['business',school,'broadcast-targets',id,page],queryFn:()=>broadcast.targets(school,id,page),refetchInterval:15000})
  const events=useQuery({queryKey:['business',school,'broadcast-events',id,eventPage],queryFn:()=>broadcast.events(school,id,eventPage)})
  async function run(job:()=>Promise<unknown>) {if(lock.current)return;lock.current=true;setBusy(true);setError('');try{await job();await refresh()}catch(e){setError((e as Error).message)}finally{lock.current=false;setBusy(false)}}
  async function act(operation:Parameters<typeof broadcast.action>[2],input:BroadcastAction={}) {
    const signature=JSON.stringify([operation,input]);let key=keys.current.get(signature);if(!key){key=crypto.randomUUID();keys.current.set(signature,key)}
    await broadcast.action(school,id,operation,key,input);keys.current.delete(signature)
  }
  const t=task.data
  if(!t)return <Alert type={task.error?'error':'info'} title={task.error?.message??'正在读取任务…'}/>
  return <Card title={<>{taskLabel(t)} · 任务详情</>}>{dialogs.contextHolder}
    <pre className="message-preview">{t.content}</pre><p>{new TextEncoder().encode(t.content).length} 字节 · {t.total} 群 · {t.batch_count} 个群主批次</p>
    <p>目标清单分页展示；确认将涵盖全部 {t.total} 个群。执行成员须使用企业微信 4.1.10 或更新版本。</p>
    {error&&<Alert type="error" title={error}/>}
    <Space wrap>
      {t.state==='draft'&&<><Button type="primary" disabled={!enabled||busy} onClick={()=>void run(async()=>{if(await dialogs.confirm(`确认正文和全部 ${t.total} 个客户群，创建企业微信任务？成员仍需确认发送。`))await act('confirm',{revision:t.revision})})}>确认创建企业微信群发任务</Button><Button disabled={busy} onClick={()=>void run(async()=>edit(id,{template_version_id:t.template_version_id,post_ids:t.post_ids,group_ids:(await allTargets(school,id)).map(x=>x.group_id),revision:t.revision,source_task_id:t.source_task_id??undefined,source_draft_id:t.source_draft_id??undefined,reason:t.resend_reason}))}>修改草稿</Button><Button disabled={busy} onClick={()=>void run(()=>act('abandon'))}>放弃草稿</Button></>}
      {t.state==='confirmed'&&<><Button disabled={busy} onClick={()=>void run(()=>act('refresh'))}>同步平台结果</Button><Button disabled={!enabled||busy||t.stopped} onClick={()=>void run(async()=>{if(await dialogs.confirm('提醒未执行成员完成群发任务？'))await act('reminders')})}>提醒成员</Button>{admin&&<><Button danger disabled={!managementAvailable||busy} onClick={()=>void run(async()=>{if(await dialogs.confirm('停止尚未执行的任务？已经发送的消息无法撤回。'))await act('stop')})}>停止后续发送</Button><Button disabled={busy} onClick={()=>void run(async()=>{const reason=await dialogs.prompt('再次发送原因（先停止并核对原任务）');if(!reason)return;const rows=(await allTargets(school,id)).filter(x=>x.official_status!==1);edit(undefined,{template_version_id:t.template_version_id,post_ids:t.post_ids,group_ids:rows.map(x=>x.group_id),source_task_id:id,reason})})}>生成再次发送草稿</Button></>}</>}
    </Space>
    {targets.error&&<Alert type="error" title={targets.error.message}/>}
    {targets.data?.items.map(x=><Card size="small" key={x.id} title={`${x.group_name} · ${x.sender}`}><p>{x.chat_id}</p>{x.msgid&&<p>官方任务：{x.msgid}</p>}<p>{targetLabel(x)}</p><p>成员任务：{x.member_status===2?'成员已执行（不代表所有群成功）':x.member_status===0?'成员未执行':'未取得成员结果'} · 最后同步：{x.last_synced_at?formatDate(x.last_synced_at):'尚未同步'}</p>{x.sync_error&&<p>同步未完成，保留上次结果</p>}{x.error_code&&<p>创建原因：{x.error_code}</p>}{x.stop_state!=='none'&&<p>停止状态：{x.stop_state}</p>}{x.reconciliation&&<p>人工核对：{x.reconciliation}（与平台事实独立）</p>}{admin&&(x.creation_state==='unknown'||x.stop_state==='unknown'||x.operation_unknown)&&<Button onClick={()=>setBatch(x.batch_id)}>核对此批次</Button>}</Card>)}
    <PaginationControls page={page} more={targets.data?.page?.has_more} busy={targets.isFetching} change={setPage} subject="目标群"/>
    {admin&&batch&&<Card title="核对创建结果不确定的批次"><p>没有查到记录不能证明未创建。请结合企业微信管理端证据，未核对前禁止再次发送。</p><Select aria-label="核对结论" value={decision} options={[{value:'not_created',label:'有证据确认未创建'},{value:'sent',label:'人工确认已发送（仍保留未知平台状态）'},{value:'linked',label:'关联已确认的官方任务 ID'},{value:'stop_confirmed',label:'有证据确认停止已生效'},{value:'reminder_reviewed',label:'提醒已核对（不重新发送）'}]} onChange={setDecision}/><Input aria-label="官方任务 ID" placeholder="关联时填写 msgid" value={msgid} onChange={e=>setMsgid(e.target.value)}/><Input.TextArea aria-label="核对依据" placeholder="核对依据" maxLength={1000} value={reason} onChange={e=>setReason(e.target.value)}/><Button disabled={busy||!reason.trim()} onClick={()=>void run(()=>act('reconciliations',{batch_id:batch,decision,reason,...(decision==='linked'?{msgid}:{})}))}>保存核对依据</Button>{platformAdmin&&<Button disabled={busy} onClick={()=>void run(async()=>{const end=Math.floor(Date.now()/1000);setCandidates(await broadcast.candidates(end-86400,end))})}>查询企业最近一天候选任务</Button>}{candidates.map(c=><p key={c.id}>{c.id} · {JSON.stringify(c.text)}（候选不代表匹配）</p>)}</Card>}
    <h3>操作记录</h3>{events.error&&<Alert type="error" title={events.error.message}/>}{events.data?.items.map(e=><p key={e.id}>{formatDate(e.created_at)} · {e.action} · {JSON.stringify(e.detail)}</p>)}<PaginationControls page={eventPage} more={events.data?.page?.has_more} busy={events.isFetching} change={setEventPage} subject="操作记录"/>
  </Card>
}
function History({school,admin,migrate}:{school:string;admin:boolean;migrate:(input:BroadcastDraftInput)=>void}) {
  const [page,setPage]=useState(1),[draftPage,setDraftPage]=useState(1),[error,setError]=useState('');const dialogs=useDialogs()
  const records=useQuery({queryKey:['business',school,'old-attempts',page],queryFn:()=>workspace.list<Attempt>(school,'delivery-attempts',`?page[number]=${page}&page[size]=30`)})
  const drafts=useQuery({queryKey:['business',school,'old-drafts',draftPage],queryFn:()=>workspace.list<Draft>(school,'delivery-drafts',`?page[number]=${draftPage}&page[size]=30`)})
  return <>{dialogs.contextHolder}<p>历史客户端结果不是官方送达证据。旧分享入口已停用；历史快照保留用于核对和防重复。</p>{error&&<Alert type="error" title={error}/>}{[records,drafts].map((q,i)=>q.error&&<Alert key={i} type="error" title={q.error.message}/>)}
    {records.data?.items.map(r=><Card key={r.id} title={String(r.group_name??r.id)}><p>{String(r.content??'')}</p><p>{r.state} · 人工核对：{r.reconciliation??'尚未核对'}</p>{admin&&<Space>{(['operator_confirmed','operator_rejected'] as const).map(decision=><Button key={decision} onClick={async()=>{const reason=await dialogs.prompt('填写核对依据');if(!reason)return;try{await workspace.mutate(school,`delivery-attempts/${r.id}/reconciliations`,{decision,reason});await records.refetch()}catch(e){setError((e as Error).message)}}}>{decision==='operator_confirmed'?'确认已发送':'确认未发送'}</Button>)}</Space>}</Card>)}
    <PaginationControls page={page} more={records.data?.page?.has_more} busy={records.isFetching} change={setPage} subject="历史记录"/>
    <h3>历史草稿快照</h3>{drafts.data?.items.map(d=><Card key={d.id} title={d.group_name}><pre>{d.content}</pre><p>旧快照仅供核对。新任务会重新选择内容、核验版本及生成正文。</p><Button onClick={()=>migrate({template_version_id:d.template_version_id,group_ids:[d.group_id],post_ids:[],source_draft_id:d.id})}>转为群发草稿</Button></Card>)}
    <PaginationControls page={draftPage} more={drafts.data?.page?.has_more} busy={drafts.isFetching} change={setDraftPage} subject="历史草稿"/>
  </>
}

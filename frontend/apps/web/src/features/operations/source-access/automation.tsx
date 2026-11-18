import {PhoneDeliveries} from './phone-deliveries'
import { PaginationControls } from '../../../shared/pagination-controls'
import { formatDate } from '../../../shared/format-date'
import { TableText, TableIdentifier } from '../../../shared/table-text'
import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Select,Space,Table,Tag,Timeline,Typography} from 'antd'
import {businessMessages,receiptReasonLabels,type BusinessMessage,type MessageDiagnostic,type MessageProgressFilter} from './api'

const channels:Record<string,string>={mobile:'手机',wecom:'企业微信'}
const states:Record<string,string>={handoff:'内部交接中',checking:'内部检查中',creating_tasks:'创建通道任务中',dispatching:'下发中',sending:'发送中',partial:'部分完成',completed:'全部成功',failed:'全部失败',finished_with_failures:'已结束（部分失败）',needs_review:'结果待核对',not_dispatched:'未下发成功',unobserved:'进度暂不可确认'}
const stages:Record<string,string>={handoff:'内部交接',checking:'内部检查',creating_tasks:'创建通道任务',scheduling:'手机调度',device_receipt:'设备接收',device_execution:'设备执行',official_queue:'企业微信排队',official_call:'企业微信调用',official_result:'官方结果',sent:'已发送',failed:'失败',unknown:'结果不确定',missing_evidence:'缺少观测'}
const owners:Record<string,string>={system:'系统',device:'手机',official:'企业微信',admin:'系统管理员'}
function elapsed(since:string|null,now:string){
 if(!since)return '暂无进展时间'
 const seconds=Math.max(0,Math.floor((Date.parse(now)-Date.parse(since))/1000))
 return seconds<60?`${seconds} 秒`:`${Math.floor(seconds/60)} 分 ${seconds%60} 秒`
}
function Diagnostic({value:d,now}:{value:MessageDiagnostic;now:string}){
 return <div><Tag color={d.severity==='attention'||d.severity==='unobserved'?'warning':d.stage==='failed'?'error':d.stage==='sent'?'success':undefined}>{d.channel?`${channels[d.channel]??d.channel} · `:''}{d.label}{d.affected_targets>0?` · ${d.affected_targets} 项`:''}</Tag>
 <div>{d.explanation}</div><Typography.Text type="secondary">原因：{d.reason_code}；</Typography.Text><Typography.Text type="secondary">最近进展距今 {elapsed(d.since,now)}；下一步：{d.next_action}（{owners[d.owner]??d.owner}）</Typography.Text>
 {d.retry_at&&d.severity!=='terminal'&&<div>调度/重试时间：{formatDate(d.retry_at)}</div>}
 {d.deadline_at&&d.severity!=='terminal'&&<div>结果截止时间：{formatDate(d.deadline_at)}</div>}
 {d.historical_risk&&<div><Tag color="warning">历史尝试结果不确定，存在重复风险；当前候选继续处理</Tag></div>}</div>
}
function MessageDetail({row,refresh}:{row:BusinessMessage;refresh:()=>void}){
 const [page,setPage]=useState(1)
 const q=useQuery({queryKey:['message-deliveries',row.receipt_id,row.progress.version,page],queryFn:()=>businessMessages.deliveries(row.receipt_id,page),refetchInterval:5000})
 const mismatch=q.data&&q.data.version!==row.progress.version
 return <Space orientation="vertical" style={{width:'100%'}}>
  {row.business_unit_id&&<PhoneDeliveries receipt={row.receipt_id}/>}
  <Button href={`/admin?view=mobile-dispatch-records&receipt_id=${encodeURIComponent(row.receipt_id)}`}>手机分发下发记录</Button>
  <Typography.Text type="secondary">观察时间：{formatDate(row.progress.observed_at)}；持久汇总时间：{row.progress.results_as_of?formatDate(row.progress.results_as_of):'尚未汇总'}</Typography.Text>
  <Typography.Paragraph style={{whiteSpace:'pre-wrap'}}>{row.content}</Typography.Paragraph>
  <Timeline items={[{content:`已持久接收：${formatDate(row.milestones.received_at)}`},...(row.milestones.handoff_completed_at?[{content:`内部交接完成：${formatDate(row.milestones.handoff_completed_at)}`}]:[]),...(row.milestones.delegated_at?[{content:`通道已受理（任务创建完成）：${formatDate(row.milestones.delegated_at)}`}]:[]),{content:`当前进度：${states[row.state]??row.state}；各通道并行执行`}]} />
  {row.progress.diagnostics.map((d,i)=><Diagnostic key={i} value={d} now={row.progress.observed_at}/>)}
  {q.error&&<Alert type="error" title={`目标明细读取失败：${q.error.message}`} action={<Button onClick={()=>void q.refetch()}>重试</Button>}/>}
  {mismatch&&<Alert type="warning" title="目标进度已更新，当前明细与列表属于不同版本" description="请同步刷新后核对汇总。" action={<Button onClick={()=>{refresh();void q.refetch()}}>同步刷新</Button>}/>}
  <Table rowKey="id" tableLayout="fixed" scroll={{x:1100}} dataSource={q.data?.items??[]} pagination={false} columns={[
   {title:'通道',width:90,render:(_,r)=>channels[r.channel]??r.channel},
   {title:'目标群',width:150,dataIndex:'target_name'},
   {title:'当前阶段与原因',width:440,render:(_,r)=><Diagnostic value={r.diagnostic} now={q.data?.observed_at??row.progress.observed_at}/>},
   {title:'当前执行身份',width:200,render:(_,r)=><><TableText value={r.device_name??r.sender??'尚未选择执行身份'}/>{r.device_id&&<TableIdentifier value={r.device_id}/>} {r.attempt_no&&<div>第 {r.attempt_no} 次尝试</div>}</>},
   {title:'执行记录',width:170,render:(_,r)=>r.task_id?<><Button type="link" href={`/admin?view=${r.channel==='mobile'?'mobile-execution-tasks':'broadcast-tasks'}&task=${encodeURIComponent(r.task_id)}`}>查看任务与尝试</Button><TableIdentifier value={r.task_id}/></>:'尚未创建通道任务'},
  ]}/>
  {!q.isPending&&!q.error&&q.data?.items.length===0&&<Typography.Text>尚未生成目标投递，请查看上方内部处理阶段。</Typography.Text>}
  <PaginationControls page={page} more={q.data?.has_more} busy={q.isFetching} change={setPage} subject="群投递明细"/>
 </Space>
}
export function SourceAutomationTasks(){
 const [page,setPage]=useState(1),[filter,setFilter]=useState<MessageProgressFilter>({}),route=new URLSearchParams(window.location.search).get('route')??''
 const q=useQuery({queryKey:['business-messages',route,page,filter],queryFn:()=>businessMessages.list(route,'',page,30,filter),refetchInterval:5000})
 const refresh=()=>{void q.refetch()}
 function choose(key:keyof MessageProgressFilter,value:string|undefined){setFilter({...filter,[key]:value});setPage(1)}
 return <Card title="即时消息与发送结果" extra={<Button loading={q.isFetching} onClick={refresh}>刷新</Button>}><Space orientation="vertical" style={{width:'100%'}}>
 <Alert type="info" title="每条消息独立处理" description="按通道 × 目标群统计投递数量；手机换机不增加群投递数。通道受理只表示任务已创建，当前进度以目标执行事实为准。没有目标群的消息显示未下发成功。"/>
 <Space wrap>
  <Select aria-label="筛选发送通道" allowClear placeholder="所有通道" style={{width:150}} value={filter.channel} onChange={v=>choose('channel',v)} options={Object.entries(channels).map(([value,label])=>({value,label}))}/>
  <Select aria-label="筛选处理阶段" allowClear placeholder="所有阶段" style={{width:180}} value={filter.stage} onChange={v=>choose('stage',v)} options={Object.entries(stages).map(([value,label])=>({value,label}))}/>
  <Select aria-label="筛选关注类型" allowClear placeholder="所有关注类型" style={{width:180}} value={filter.attention} onChange={v=>choose('attention',v)} options={[{value:'normal',label:'正常处理'},{value:'retrying',label:'自动重试'},{value:'attention',label:'需要关注'},{value:'terminal',label:'已结束'},{value:'unobserved',label:'缺少观测'},{value:'projection_lag',label:'汇总更新滞后'}]}/>
 </Space>
 {q.error&&<Alert type="error" title={q.error.message}/>}
 <Table tableLayout="fixed" scroll={{x:1450}} rowKey="id" dataSource={q.data?.items??[]} pagination={false} expandable={{expandedRowRender:r=><MessageDetail row={r} refresh={refresh}/>}} columns={[
  {title:'消息 ID',width:180,dataIndex:'event_id',render:value=><TableIdentifier value={value}/>},
  {title:'消息进度',width:150,render:(_,r)=>states[r.state]??r.state},
  {title:'目标投递（通道 × 群）',width:230,render:(_,r)=><><div>{r.progress.total===null?'目标尚未生成':`共 ${r.progress.total} 项`}{!r.progress.counts_complete&&<Tag color="warning">投递统计待核对</Tag>}</div>{r.sending?<><div>成功 {r.sending.sent} / 失败 {r.sending.failed}</div><div>处理中 {r.sending.pending} / 不确定 {r.sending.unknown}</div></>:<TableText value={r.reason_code?(receiptReasonLabels[r.reason_code]??r.reason_code):'尚无目标结果'}/>}</>},
  {title:'当前关注 · 等待位置与下一步',width:620,render:(_,r)=><>{r.progress.diagnostics.slice(0,2).map((d,i)=><Diagnostic key={i} value={d} now={r.progress.observed_at}/>)}{r.progress.diagnostics.length>2&&<div>另有 {r.progress.diagnostics.length-2} 类，展开查看</div>}{r.progress.projection_lag&&<Tag color="warning">持久汇总待更新；当前显示实时目标事实</Tag>}</>},
  {title:'接收时间',width:180,dataIndex:'created_at',render:value=>formatDate(value)},
 ]}/>
 <PaginationControls page={page} more={q.data?.has_more} busy={q.isFetching} change={setPage} subject="即时消息与发送结果"/>
 </Space></Card>
}

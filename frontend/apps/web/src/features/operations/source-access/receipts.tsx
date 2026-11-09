import {KeywordChecks} from './keyword-checks'
import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Descriptions,Drawer,Input,Modal,Space,Table,Typography} from 'antd'
import {sourceReceipts,receiptStatusLabels,receiptStageLabels,receiptReasonLabels,routingIssueLabels,type ReceiptRow} from './api'
const status=(value:string)=>receiptStatusLabels[value]??value
const stage=(value:string)=>receiptStageLabels[value]??value
const reason=(value:string|null)=>value?(receiptReasonLabels[value]??routingIssueLabels[value]??value):'—'
export function SourceReceipts({route}:{route:string}){
 const [page,setPage]=useState(1),[receipt,setReceipt]=useState<string|null>(null),[error,setError]=useState(''),[confirm,setConfirm]=useState(false),[note,setNote]=useState(''),[busy,setBusy]=useState(false)
 const list=useQuery({queryKey:['source-receipts',route,page],queryFn:()=>sourceReceipts.list(route,page),refetchInterval:10000})
 const detail=useQuery({queryKey:['source-receipt',route,receipt],queryFn:()=>sourceReceipts.detail(route,receipt!),enabled:!!receipt,refetchInterval:receipt?10000:false})
 const refresh=async()=>{await Promise.all([list.refetch(),...(receipt?[detail.refetch()]:[])])}
 const recheck=async()=>{if(!detail.data||!note.trim()){setError('请填写重检原因');return}setBusy(true);setError('');try{await sourceReceipts.recheck(route,detail.data.id,detail.data.revision,note);setConfirm(false);await refresh()}catch(e){setError((e as Error).message);await detail.refetch()}finally{setBusy(false)}}
 const summary=(r:ReceiptRow)=>r.sending?`待处理 ${r.sending.pending} / 已发送 ${r.sending.sent} / 失败 ${r.sending.failed} / 结果未知 ${r.sending.unknown}`:'尚无发送摘要'
 return <Space orientation="vertical" style={{width:'100%'}}>
  <Alert type="info" title="收件成功与素材应用不代表已发群；发送摘要按实际目标结果统计，手机换机不增加目标数。"/>
  {(error||list.error||detail.error)&&<Alert type="error" title={error||list.error?.message||detail.error?.message}/>}
  <Button onClick={refresh}>刷新收件</Button>
  <Table<ReceiptRow> rowKey="id" loading={list.isFetching} dataSource={list.data?.items??[]} pagination={false} columns={[
   {title:'事件 ID',dataIndex:'event_id'},{title:'内容 ID',dataIndex:'external_content_id'},{title:'版本序号',dataIndex:'sequence'},
   {title:'素材状态',render:(_,r)=>status(r.status)},{title:'处理阶段',render:(_,r)=>stage(r.processing_stage)},
   {title:'原因',render:(_,r)=>reason(r.reason_code)},{title:'发送结果',render:(_,r)=>summary(r)},{title:'接收时间',dataIndex:'received_at'},
   {title:'操作',render:(_,r)=><Button onClick={()=>{setError('');setReceipt(r.id)}}>查看回执</Button>},
  ]}/>
  <Space><Button disabled={page===1} onClick={()=>setPage(page-1)}>上一页</Button><Typography.Text>{page}</Typography.Text><Button disabled={!list.data?.has_more} onClick={()=>setPage(page+1)}>下一页</Button></Space>
  <Drawer open={!!receipt} title="收件与回执详情" size="large" onClose={()=>{setReceipt(null);setConfirm(false)}}>
   {detail.data&&<Space orientation="vertical" style={{width:'100%'}}>
    <Descriptions column={1} items={[
     {key:'receipt',label:'回执 ID',children:detail.data.id},{key:'event',label:'事件 ID',children:detail.data.event_id},
     {key:'state',label:'素材状态',children:status(detail.data.status)},{key:'stage',label:'处理阶段',children:stage(detail.data.processing_stage)},
     {key:'reason',label:'处理原因',children:reason(detail.data.reason_code)},{key:'sequence',label:'已观察水位',children:detail.data.observed_sequence??'—'},
     {key:'sending',label:'发送结果',children:summary(detail.data)},{key:'updated',label:'最后更新时间',children:detail.data.updated_at},
    ]}/>
    {detail.data.can_recheck&&<Button disabled={!['keyword_blocked','workspace_keyword_blocked','final_keyword_blocked'].includes(detail.data.reason_code??'')} onClick={()=>{setNote('');setConfirm(true)}}>按当前规则重检</Button>}
    <Typography.Title level={5}>处理时间线（最近 100 条）</Typography.Title>
    <Table rowKey="revision" dataSource={detail.data.history} pagination={false} columns={[{title:'处理修订',dataIndex:'revision'},{title:'状态',render:(_,h)=>status(h.status)},{title:'阶段',render:(_,h)=>stage(h.stage)},{title:'原因',render:(_,h)=>reason(h.reason_code)},{title:'时间',dataIndex:'created_at'}]}/>
    <KeywordChecks key={detail.data.id} route={route} receipt={detail.data.id}/>
    <Typography.Title level={5}>历史检查记录（旧算法，最近 100 条）</Typography.Title>
    <Table rowKey={r=>`${r.rule_revision}:${r.fingerprint}`} dataSource={detail.data.checks} pagination={false} columns={[{title:'规则修订',dataIndex:'rule_revision'},{title:'结论',render:(_,r)=>r.outcome==='passed'?'通过':'拦截'},{title:'时间',dataIndex:'created_at'}]}/>
    <Typography.Title level={5}>版本关联</Typography.Title>
    <Table rowKey={r=>`${r.kind}:${r.related_receipt_id}`} dataSource={detail.data.relations} pagination={false} columns={[{title:'关系',render:(_,r)=>({duplicate:'等价重复',update:'替代旧版',withdrawal:'撤回旧版'}[r.kind])},{title:'关联回执',dataIndex:'related_receipt_id'}]}/>
   </Space>}
  </Drawer>
  <Modal open={confirm} title="按当前规则重新检查原内容" okText="提交重检" cancelText="取消" confirmLoading={busy} onCancel={()=>setConfirm(false)} onOk={recheck}>
   <Typography.Paragraph>保留原回执与检查历史。重检不强制放行，不会重复创建已有发送任务。</Typography.Paragraph>
   <Input.TextArea aria-label="重检原因" maxLength={2000} value={note} onChange={e=>setNote(e.target.value)}/>
   {error&&<Alert type="error" title={error}/>}
  </Modal>
 </Space>
}

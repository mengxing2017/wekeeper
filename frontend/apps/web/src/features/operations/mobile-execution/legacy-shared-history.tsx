import {useRef,useState} from 'react'
import {useMutation} from '@tanstack/react-query'
import {Alert,Button,Input,Modal,Select,Table} from 'antd'
import {legacySharedSend,type LegacySharedTask} from './api'
import {createIdempotencyKey} from '../../../shared/idempotency-key'

/** Only existing v8 work can be inspected, stopped or reviewed; no new send action. */
export function LegacySharedHistory({school,admin}:{school:string;admin:boolean}){
 const keys=useRef(new Map<string,string>());function keyFor(input:unknown){const value=JSON.stringify(input);let key=keys.current.get(value);if(!key){key=createIdempotencyKey();keys.current.set(value,key)}return key}
 const [detail,setDetail]=useState<LegacySharedTask>();const [reason,setReason]=useState('')
 const [review,setReview]=useState<{id:string;revision:number}>();const [decision,setDecision]=useState<'still_unknown'|'verified_not_sent'|'manually_confirmed_sent'>('still_unknown')
 const list=useMutation({mutationFn:()=>legacySharedSend.overview(school)})
 const read=useMutation({mutationFn:(id:string)=>legacySharedSend.task(school,id),onSuccess:setDetail})
 const stop=useMutation({mutationFn:()=>legacySharedSend.stop(school,detail!.id,detail!.revision,reason,keyFor(['stop',detail!.id,detail!.revision,reason])),onSuccess:t=>{setDetail(t);setReason('');list.mutate()}})
 const check=useMutation({mutationFn:async()=>{await legacySharedSend.review(school,review!.id,review!.revision,decision,reason,keyFor(['review',review,decision,reason]));return legacySharedSend.task(school,detail!.id)},onSuccess:t=>{setDetail(t);setReview(undefined);setReason('')}})
 const error=list.error||read.error||stop.error||check.error
 return <>
  <Button loading={list.isPending} onClick={()=>list.mutate()}>查询历史 v8 任务与风险</Button>
  {error&&<Alert type="error" title={error.message}/>}
  {list.data&&<><p>历史账本保留原结果，不升级为新计划。只停止新许可或按真实证据核对；不能从历史入口创建、确认或改派发送。</p><Table<LegacySharedTask> rowKey="id" dataSource={list.data.tasks} columns={[{title:'创建时间',dataIndex:'created_at'},{title:'状态',render:(_,t)=>t.stopped?'已停止':'历史任务'},{title:'操作',render:(_,t)=><Button onClick={()=>read.mutate(t.id)}>查看历史结果</Button>}]}/></>}
  <Modal title="历史 v8 任务收尾" open={!!detail} footer={null} onCancel={()=>{setDetail(undefined);setReason('')}}>
   {detail?.items?.map(i=><div key={i.id}>{i.attempts.map(a=><p key={a.id}>{a.plan.search_text} / {a.plan.device_name} / {a.state}{admin&&a.current&&a.state==='unknown'&&<Button onClick={()=>{setReview({id:a.id,revision:a.revision});setDecision('still_unknown');setReason('')}}>核对历史不确定结果</Button>}</p>)}</div>)}
   {admin&&detail&&!detail.stopped&&<><Input.TextArea aria-label="停止历史任务理由" value={reason} onChange={e=>setReason(e.target.value)}/><Button danger disabled={!reason.trim()} loading={stop.isPending} onClick={()=>stop.mutate()}>停止历史未许可工作</Button></>}
  </Modal>
  <Modal title="核对历史不确定结果" open={!!review} confirmLoading={check.isPending} okButtonProps={{disabled:!reason.trim()}} onCancel={()=>setReview(undefined)} onOk={()=>check.mutate()}>
   <p>超时不代表未发送。旧物理占用、固定结果及全部证据继续保留。</p><Select value={decision} onChange={setDecision} options={[{value:'still_unknown',label:'仍不确定，保持阻断'},{value:'verified_not_sent',label:'有充分依据确认未发送'},{value:'manually_confirmed_sent',label:'有充分依据确认已发送'}]}/><Input.TextArea aria-label="历史结果核对依据" value={reason} onChange={e=>setReason(e.target.value)}/>
  </Modal>
 </>
}

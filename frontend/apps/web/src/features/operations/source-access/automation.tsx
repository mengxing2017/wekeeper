import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Descriptions,Drawer,Input,Modal,Select,Space,Table,Tabs,Typography} from 'antd'
import {sourceAutomation,executionEvidenceLabels as evidenceLabels,automationStateLabels as labels,type AutomationTask,type AutomationTarget,type AutomationFilter,type VersionDeliveryPolicy} from './api'
export function SourceAutomationTasks(){
 const initial=new URLSearchParams(window.location.search)
 const [filter,setFilter]=useState<AutomationFilter>({route_id:initial.get('route')??'',channel:initial.get('channel')??'',workspace_id:initial.get('workspace_id')??'',page:1})
 const [id,setId]=useState<string|null>(initial.get('task')),[target,setTarget]=useState<string|null>(null),[cursor,setCursor]=useState(0),[previous,setPrevious]=useState<number[]>([])
 const [detailSection,setDetailSection]=useState(initial.get('section')==='evidence'?'evidence':'overview')
 const [observationCursor,setObservationCursor]=useState(0),[observationPrevious,setObservationPrevious]=useState<number[]>([])
 const [action,setAction]=useState<'stop'|'retry'|null>(null),[reason,setReason]=useState(''),[key,setKey]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('')
 const list=useQuery({queryKey:['source-automation',filter],queryFn:()=>sourceAutomation.list(filter),refetchInterval:10000})
 const detail=useQuery({queryKey:['source-automation-detail',id],queryFn:()=>sourceAutomation.detail(id!),enabled:!!id,refetchInterval:id?10000:false})
 const targets=useQuery({queryKey:['source-automation-targets',id],queryFn:()=>sourceAutomation.targets(id!),enabled:!!id,refetchInterval:id?10000:false})
 const events=useQuery({queryKey:['source-automation-events',id,cursor],queryFn:()=>sourceAutomation.events(id!,cursor),enabled:!!id,refetchInterval:id?10000:false})
 const observations=useQuery({queryKey:['source-automation-observations',id,observationCursor],queryFn:()=>sourceAutomation.observations(id!,observationCursor),enabled:!!id,refetchInterval:id?10000:false})
 const attempts=useQuery({queryKey:['source-automation-attempts',id,target],queryFn:()=>sourceAutomation.attempts(id!,target!),enabled:!!id&&!!target,refetchInterval:target?10000:false})
 const open=(v:string,section='overview')=>{setDetailSection(section);setId(v);setTarget(null);setCursor(0);setPrevious([]);setObservationCursor(0);setObservationPrevious([])}
 const change=(k:keyof AutomationFilter,v:string)=>setFilter(f=>({...f,[k]:v,page:1}))
 const perform=async()=>{if(!id||!detail.data||!action||!reason.trim()){setError('请填写操作原因');return}setBusy(true);setError('');try{const input={expected_revision:detail.data.task.revision,idempotency_key:key,reason};await (action==='stop'?sourceAutomation.stop(id,input):sourceAutomation.retry(id,input));setAction(null);await Promise.all([detail.refetch(),targets.refetch(),events.refetch(),list.refetch()])}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 const task=detail.data?.task
 const executionEvidence=<Space orientation="vertical" style={{width:'100%'}}>    <Typography.Title level={5}>执行证据</Typography.Title>
    <Table rowKey="id" dataSource={observations.data?.items??[]} pagination={false} columns={[{title:'证据级别',render:(_,r)=>evidenceLabels[r.evidence_type]??r.evidence_type},{title:'来源',dataIndex:'observation_source'},{title:'观察时间',dataIndex:'observed_at'},{title:'目标 / 尝试',render:(_,r)=>r.target_id??r.attempt_id??r.batch_id??'任务受理'},{title:'设备本地时间（未校时）',dataIndex:'device_occurred_at'}]}/>
    <Space><Button disabled={!observationPrevious.length} onClick={()=>{setObservationCursor(observationPrevious[observationPrevious.length-1]);setObservationPrevious(observationPrevious.slice(0,-1))}}>上一段证据</Button><Button disabled={observations.data?.next_cursor==null} onClick={()=>{setObservationPrevious([...observationPrevious,observationCursor]);setObservationCursor(observations.data!.next_cursor!)}}>下一段证据</Button></Space>
</Space>
 return <Card title="自动链路任务"><Space orientation="vertical" style={{width:'100%'}}>
  <Alert type="info" title="任务由已授权链路自动执行。受理不代表送达；停止只控制后续可控提交，已交付或在途结果继续入账。"/>
  {(error||list.error||detail.error||targets.error||events.error||observations.error||attempts.error)&&<Alert type="error" title={error||list.error?.message||detail.error?.message||targets.error?.message||events.error?.message||observations.error?.message||attempts.error?.message}/>}
  {filter.workspace_id&&<Space><Typography.Text>已限定工作空间：{filter.workspace_id}</Typography.Text><Button onClick={()=>change('workspace_id','')}>查看全部授权工作空间</Button></Space>}
  <Space wrap>{(['source_id','unit_id','route_id'] as const).map(k=><Input key={k} aria-label={{source_id:'来源 ID',unit_id:'单位 ID',route_id:'路由 ID'}[k]} placeholder={{source_id:'来源 ID',unit_id:'单位 ID',route_id:'路由 ID'}[k]} value={filter[k]} onChange={e=>change(k,e.target.value)}/>)}
   <Select aria-label="执行通道" allowClear placeholder="全部通道" style={{width:150}} value={filter.channel||undefined} onChange={v=>change('channel',v??'')} options={[{value:'wecom',label:'企业微信'},{value:'mobile',label:'移动端'}]}/>
   <Select aria-label="任务状态" allowClear placeholder="全部状态" style={{width:150}} value={filter.state||undefined} onChange={v=>change('state',v??'')} options={Object.entries(labels).filter(([k])=>!['sent','dispatching','retry_wait','cancelled'].includes(k)).map(([value,label])=>({value,label}))}/>
   <Input aria-label="开始时间" placeholder="开始时间，如 2026-11-10T00:00:00+08:00" value={filter.from} onChange={e=>change('from',e.target.value)}/><Input aria-label="结束时间" placeholder="结束时间（含时区）" value={filter.until} onChange={e=>change('until',e.target.value)}/>
   <Button onClick={()=>list.refetch()}>刷新任务</Button>
  </Space>
  <Table<AutomationTask> rowKey="id" loading={list.isFetching} dataSource={list.data?.items??[]} pagination={false} columns={[
   {title:'内容 ID',dataIndex:'external_content_id'},{title:'路由',dataIndex:'route_id'},{title:'通道',render:(_,r)=>r.channel==='mobile'?'移动端':'企业微信'},
   {title:'状态',render:(_,r)=>labels[r.state]??r.state},{title:'目标结果',render:(_,r)=>`已发 ${r.counts.sent} / 待处理 ${r.counts.pending} / 失败 ${r.counts.failed} / 不确定 ${r.counts.unknown} / 停止或取消 ${r.counts.stopped+r.counts.cancelled}`},
   {title:'创建时间',dataIndex:'created_at'},{title:'操作',render:(_,r)=><Space><Button onClick={()=>open(r.id)}>任务详情</Button><Button onClick={()=>open(r.id,'evidence')}>执行证据</Button></Space>},
  ]}/>
  <Space><Button disabled={(filter.page??1)===1} onClick={()=>setFilter({...filter,page:(filter.page??1)-1})}>上一页</Button><Typography.Text>{filter.page??1}</Typography.Text><Button disabled={!list.data?.has_more} onClick={()=>setFilter({...filter,page:(filter.page??1)+1})}>下一页</Button></Space>
  <Drawer title="自动任务详情" open={!!id} size="large" onClose={()=>{setId(null);setTarget(null)}}>
   {detail.data&&task&&<Tabs activeKey={detailSection} onChange={setDetailSection} items={[{key:'overview',label:'任务概览',children:<Space orientation="vertical" style={{width:'100%'}}>
    {task.stopped&&<Alert type="warning" title={`已停止后续执行：${task.stop_reason??'停止请求已记录'}。已发送和不确定事实保留。`}/>}
    {task.has_unknown&&<Alert type="warning" title="存在不确定尝试，手机候选切换可能产生跨手机重复；迟到结果仍归原尝试。"/>}
    {detail.data.execution&&<Alert type="info" title={detail.data.execution.external_confirmation==='member_required'?'企业微信客户群：平台自动创建并同步；成员在微信侧确认，无需平台逐条人工确认。':'手机按完整签名包本地执行；成功表示设备反馈，不代表官方验证送达。'}/>}
    <Descriptions column={1} items={[
     {key:'id',label:'任务 ID',children:task.id},{key:'decision',label:'冻结决策',children:task.decision_id},{key:'revision',label:'路由修订',children:task.route_revision},
     {key:'state',label:'状态',children:labels[task.state]},{key:'executor',label:'执行身份',children:detail.data.frozen.executed_by},{key:'authorizer',label:'授权人',children:detail.data.frozen.authorized_by},
     {key:'policy',label:'冻结版本策略',children:detail.data.frozen.version_policy?`${detail.data.frozen.version_policy.policy==='resend_new_version'?'后续新版本再投':'默认不重复群发'} / 修订 ${detail.data.frozen.version_policy.revision}`:'历史未记录'},
     {key:'reconstructed',label:'历史回填',children:detail.data.frozen.reconstructed?'由历史事实重建':'否'},
    ]}/>
    <Typography.Title level={5}>首次冻结正文</Typography.Title><Typography.Paragraph style={{whiteSpace:'pre-wrap'}}>{detail.data.frozen.content}</Typography.Paragraph>
    {detail.data.composition&&<>{detail.data.composition.content!==detail.data.frozen.content&&<><Typography.Title level={5}>当前代次正文</Typography.Title><Typography.Paragraph style={{whiteSpace:'pre-wrap'}}>{detail.data.composition.content}</Typography.Paragraph></>}<Typography.Title level={5}>广告与规则依据</Typography.Title><Typography.Paragraph style={{whiteSpace:'pre-wrap'}}>{JSON.stringify(detail.data.composition.selection,null,2)}</Typography.Paragraph><Typography.Text>原文检查 {detail.data.composition.common_check} / 工作空间检查 {detail.data.composition.workspace_check} / 最终检查 {detail.data.composition.final_check}</Typography.Text></>}
    <Space><Button danger disabled={!task.can_stop} onClick={()=>{setAction('stop');setReason('');setKey(crypto.randomUUID());setError('')}}>停止任务</Button><Button disabled={!task.can_retry} onClick={()=>{setAction('retry');setReason('');setKey(crypto.randomUUID());setError('')}}>推进可恢复目标</Button><Button href={`/admin?view=source-receipts&route=${encodeURIComponent(task.route_id)}`}>收件与回执</Button></Space>
    <Typography.Title level={5}>目标投递</Typography.Title>
    <Table<AutomationTarget> rowKey="target_id" dataSource={targets.data?.items??[]} pagination={false} columns={[{title:'群',dataIndex:'name'},{title:'状态',render:(_,r)=>labels[r.state]??r.state},{title:'原因',dataIndex:'reason'},{title:'下次执行',dataIndex:'next_attempt_at'},{title:'操作',render:(_,r)=><Button onClick={()=>setTarget(r.target_id)}>执行尝试</Button>}]}/>
    {!!targets.data?.excluded.length&&<Alert type="warning" title="冻结时已排除的目标" description={targets.data.excluded.map(t=>`${t.target_id}：${t.reason}`).join('；')}/>}
    <Typography.Title level={5}>阶段轨迹</Typography.Title>
    <Table rowKey="id" dataSource={events.data?.items??[]} pagination={false} columns={[{title:'阶段',dataIndex:'kind'},{title:'依据',render:(_,r)=>JSON.stringify(r.detail)},{title:'时间',dataIndex:'created_at'}]}/>
    <Space><Button disabled={!previous.length} onClick={()=>{setCursor(previous[previous.length-1]);setPrevious(previous.slice(0,-1))}}>上一段轨迹</Button><Button disabled={events.data?.next_cursor==null} onClick={()=>{setPrevious([...previous,cursor]);setCursor(events.data!.next_cursor!)}}>下一段轨迹</Button></Space>
   </Space>},{key:'evidence',label:'执行证据',children:executionEvidence}]}/>}
  </Drawer>
  <Drawer title="执行尝试与外部证据" open={!!target} size="large" onClose={()=>setTarget(null)}><Table rowKey="id" dataSource={attempts.data?.items??[]} pagination={{pageSize:20}} columns={[{title:'尝试 ID',dataIndex:'id'},{title:'代次',dataIndex:'generation'},{title:'状态',dataIndex:'state'},{title:'手机/外部请求',render:(_,r)=>r.device_id??r.invocation_id??'—'},{title:'执行结果',render:(_,r)=>r.result_status??r.error_code??'尚无最终结果'},{title:'微信侧成员',render:(_,r)=>r.channel==='wecom'?(r.member_status===2?'成员已执行，逐群结果另查':r.member_status===0?'等待微信侧成员执行':'尚未取得可解释的成员结果'):'—'},{title:'设备本地完成时间（未校时）',dataIndex:'device_finished_at'},{title:'平台结果接收时间',dataIndex:'result_received_at'},{title:'外部任务身份（非送达凭证）',render:(_,r)=>r.msgid??'—'}]}/></Drawer>
  <Modal open={!!action} title={action==='stop'?'停止后续可控执行':'推进原授权范围内可恢复目标'} okText="提交操作" cancelText="取消" confirmLoading={busy} onCancel={()=>setAction(null)} onOk={perform}><Typography.Paragraph>不会重发成功目标，不重置轮次或授权截止时间，不能收回已交付执行包。</Typography.Paragraph><Input.TextArea aria-label="任务操作原因" value={reason} onChange={e=>setReason(e.target.value)} maxLength={2000}/>{error&&<Alert type="error" title={error}/>}</Modal>
 </Space></Card>
}
export function VersionPolicyEditor({route}:{route:string}){
 const policy=useQuery({queryKey:['source-version-policy',route],queryFn:()=>sourceAutomation.policy(route)})
 const [value,setValue]=useState<VersionDeliveryPolicy['policy']|null>(null),[reason,setReason]=useState(''),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const save=async()=>{if(!policy.data||!reason.trim()){setError('请填写策略变更原因');return}setBusy(true);try{await sourceAutomation.savePolicy(route,policy.data.revision,value??policy.data.policy,reason);await policy.refetch();setError('')}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 return <Space orientation="vertical"><Alert type="info" title="默认编辑已发送素材不会再次群发。显式再投仅适用于启用后的更高内容版本，旧版本结果未核清时不建立版本再投。"/>{(error||policy.error)&&<Alert type="error" title={error||policy.error?.message}/>}<Select aria-label="版本投递策略" value={value??policy.data?.policy} onChange={setValue} options={[{value:'replace_unsubmitted',label:'更新未提交引用，不重复群发'},{value:'resend_new_version',label:'明确授权后续新版本再投'}]}/><Input.TextArea aria-label="版本策略变更原因" value={reason} onChange={e=>setReason(e.target.value)} maxLength={2000}/><Button disabled={!policy.data} loading={busy} onClick={save}>保存版本投递策略</Button></Space>
}

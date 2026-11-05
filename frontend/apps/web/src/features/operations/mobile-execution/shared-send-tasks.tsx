import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Input, InputNumber, Modal, Select, Space, Table } from 'antd'
import { api, sharedSend, type SharedDraft, type SharedPreview, type SharedTask, type SharedAttempt } from './api'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { PaginationControls } from '../../../shared/pagination-controls'
import { taskError, attemptFailureReason } from './task-model'
import { previewSelections, previewResourcesChanged, isUsageBlocker, currentReadiness, dispatchLabel } from './shared-preview-model'
import { SharedReadinessView } from './shared-readiness-view'
import { UsageManagement } from '../../admin/mobile-resources/usage-management'

const stateLabels:Record<string,string>={queued:'已入队，等待执行',leased:'手机已领取，准备中',prepared:'文字准备完成，等待单次许可',invocation_issued:'已发单次许可，等待手机结果',observed_sent:'手机观察到新的已发消息，未证明对方送达',unknown:'结果不确定，必须人工核对，禁止自动重发',failed_before_call:'调用前准备失败，未发出调用许可',invalidated:'执行依据已失效',cancelled:'未执行部分已取消',verified_not_sent:'人工核对确认未发送',manually_confirmed_sent:'人工核对确认已发送'}
export function SharedSendTasks(props:{school:string;admin:boolean}) { return <SchoolSharedSendTasks key={props.school} {...props}/> }
function SchoolSharedSendTasks({ school, admin }: { school: string; admin: boolean }) {
  const cache = useQueryClient()
  const [errorModal, errorContext] = Modal.useModal()
  const requestKeys=useRef(new Map<string,string>())
  function keyFor(kind:string,input:unknown){const identity=JSON.stringify([kind,input]);let key=requestKeys.current.get(identity);if(!key){key=createIdempotencyKey();requestKeys.current.set(identity,key)}return key}
  const [action,setAction]=useState<{kind:'stop'|'review'|'reassign';attempt?:SharedAttempt}>()
  const [reason,setReason]=useState('')
  const [usageOpen,setUsageOpen]=useState(false)
  const [decision,setDecision]=useState<'still_unknown'|'verified_not_sent'|'manually_confirmed_sent'>('still_unknown')
  const [device,setDevice]=useState<string>()

  const [targetPage,setTargetPage]=useState(1)
  const [targetSearch,setTargetSearch]=useState('')
  const [targetNames,setTargetNames]=useState<Record<string,string>>({})
  const [gap,setGap]=useState(30)
  const [message, setMessage] = useState('')
  const [targets, setTargets] = useState<string[]>([])
  const [draft, setDraft] = useState<SharedDraft>()
  const [preview, setPreview] = useState<SharedPreview>()
  const [selected, setSelected] = useState<{school:string;id:string}>()
  const detailKey=['mobile-execution',school,'shared-task',selected?.school===school?selected.id:undefined]
  const detailQuery=useQuery({queryKey:detailKey,queryFn:()=>sharedSend.task(school,selected!.id),enabled:!!selected&&selected.school===school,refetchInterval:5_000,refetchOnWindowFocus:'always'})
  const detail=selected?.school===school?detailQuery.data:undefined
  function setDetail(value:SharedTask|undefined) {
    if(value){cache.setQueryData(['mobile-execution',school,'shared-task',value.id],value);setSelected({school,id:value.id})}
    else setSelected(undefined)
  }
  const bindings=useQuery({queryKey:['mobile-execution',school,'shared-reassign-bindings'],queryFn:()=>api.bindings(school,1),enabled:action?.kind==='reassign'})
  const overview = useQuery({ queryKey: ['mobile-execution', school, 'shared-tasks'], queryFn: () => sharedSend.overview(school), refetchInterval: 20_000 })
  const options = useQuery({ queryKey: ['mobile-execution', school, 'shared-targets',targetPage,targetSearch], queryFn: () => api.targets(school, targetPage,targetSearch) })
  const refresh = () => cache.invalidateQueries({ queryKey: ['mobile-execution', school] })
  async function generatePreview(selections:Array<{target_id:string;device_id:string}>) {
    const input={draft_id:draft?.id,expected_revision:draft?.revision,target_ids:targets,message,between_targets_seconds:gap,selections}
    const result=await sharedSend.taskPreview(school,input,keyFor('preview',input))
    setDraft(result.draft)
    setTargetNames(n=>({...n,...Object.fromEntries(result.draft.targets.map(t=>[t.id,t.name]))}))
    requestKeys.current.delete(JSON.stringify(['preview',input]))
    return result.preview
  }
  const inspect = useMutation({ mutationFn: () => generatePreview([]), onSuccess:p=>{setPreview(p);void refresh()} })
  const confirm = useMutation({ mutationFn: () => sharedSend.confirm(school, preview!, keyFor("confirm",preview!.id)), onError: error => { errorModal.error({ title: '确认发送失败', content: taskError(error), okText: '知道了' }) }, onSuccess: t => { setDetail(t);  setDraft(undefined); setPreview(undefined); setMessage(''); setTargets([]); void refresh() } })
  const reselect=useMutation({mutationFn:(selections:Array<{target_id:string;device_id:string}>)=>generatePreview(selections),onSuccess:setPreview})
  const mutate=useMutation({mutationFn:async()=>{
    if(action!.kind==='stop')return sharedSend.stop(school,detail!.id,detail!.revision,reason,keyFor('stop',[detail!.id,detail!.revision,reason]))
    const a=action!.attempt!
    if(action!.kind==='review'){await sharedSend.review(school,a.id,a.revision,decision,reason,keyFor('review',[a.id,a.revision,decision,reason]));return sharedSend.task(school,detail!.id)}
    return sharedSend.reassign(school,a.id,a.revision,device!,reason,keyFor('reassign',[a.id,a.revision,device,reason]))
  },onSuccess:t=>{setDetail(t);setAction(undefined);setReason('');setDevice(undefined);void refresh()}})
  const error = overview.error || options.error || inspect.error || confirm.error || detailQuery.error || reselect.error || mutate.error || bindings.error
  const dirty = !!draft && (message.replace(/\r\n?/g, '\n') !== draft.message || targets.join() !== draft.target_ids.join() || gap!==draft.between_targets_seconds)
  const synchronized=overview.isSuccess&&!overview.isError
  const liveOverview=synchronized ? overview.data : undefined
  const resourcesChanged=preview ? previewResourcesChanged(preview,liveOverview) : false
  const confirmationIssues = !admin ? ['当前账号没有本校管理员发送权限；平台管理员也需在学校成员管理中获得本校管理员角色'] : preview?.plan.items.flatMap(i => !i.selected ? [`${i.target.name}：请选择已绑定手机`] : i.blocker ? [`${i.target.name}：${i.blocker}`] : []) ?? []
  return <Card title="手机共享发送">
    {errorContext}
    <Space orientation="vertical" style={{ width: '100%' }}>
      <Alert type="info" showIcon title="按目标群创建，逐群选择共享手机" description="填写文字和目标群后直接预览，系统自动保存草稿。核对预览后确认发送。确认时同时授权预览中的有效候选，所选手机为首选；离线或调用前失败时自动选择下一候选。结果不确定时仅对应目标待核对，不自动重发。企业微信官方群发独立使用。" />
      {overview.data?.sending_status&&<Alert type={overview.data.sending_status.can_confirm?'info':'warning'} title={overview.data.sending_status.message}/>}
      {overview.isError&&<Alert type="warning" title="当前状态同步失败，预览快照不能作为实时状态" description={<Button onClick={()=>void overview.refetch()}>重试同步当前状态</Button>}/>}
      {error && <Alert type="error" title={taskError(error)} />}
      <Input.Search aria-label="搜索手机目标群" placeholder="搜索本校群名或别名" onSearch={value=>{setTargetSearch(value);setTargetPage(1)}} />
      <Select labelInValue mode="multiple" aria-label="手机目标群" placeholder="选择本校手机目标群" style={{ width: '100%' }} value={targets.map(value=>({value,label:targetNames[value] ?? draft?.targets.find(t=>t.id===value)?.name ?? '目标信息待核对'}))} onChange={values=>{setTargets(values.map(v=>v.value));setTargetNames(names=>({...names,...Object.fromEntries(values.map(v=>[v.value,String(v.label)]))}))}} options={(options.data?.items ?? []).filter(t => t.enabled && !t.archived_at).map(t => ({ value: t.id, label: t.alias || t.search_text }))} />
      <PaginationControls page={targetPage} more={options.data?.page?.has_more} change={setTargetPage} subject="可选手机目标群" />
      {!!overview.data?.drafts.length && <Select aria-label="恢复共享发送草稿" placeholder="恢复已保存草稿" style={{width:'100%'}} options={overview.data.drafts.map(d=>({value:d.id,label:`${d.message.slice(0,30)}（${d.target_ids.length} 个目标群）`}))} onChange={id=>{const d=overview.data!.drafts.find(x=>x.id===id)!;setDraft(d);setTargetNames(n=>({...n,...Object.fromEntries(d.targets.map(t=>[t.id,t.name]))}));setTargets(d.target_ids);setMessage(d.message);setGap(d.between_targets_seconds);setPreview(undefined)}} />}
      <label>逐群间隔（秒）<InputNumber aria-label="逐群间隔秒数" value={gap} min={1} max={86400} precision={0} onChange={v=>setGap(v??30)}/></label>
      <p>不同目标独立调度，遵守业务间隔；某手机离线或前群待核对不拖住无关目标。间隔由使用方案与此处设置共同约束。</p>
      <Space>{targets.map((id,index)=><span key={id}>{index+1}. {targetNames[id]??draft?.targets.find(t=>t.id===id)?.name??'目标群'} <Button disabled={index===0} onClick={()=>setTargets(ids=>{const next=[...ids];[next[index-1],next[index]]=[next[index],next[index-1]];return next})}>前移</Button></span>)}</Space>
      <Input.TextArea aria-label="发送文字" rows={5} value={message} onChange={e => setMessage(e.target.value)} placeholder="输入实际发送的文字；空格与换行会保留" />
      <Button type="primary" loading={inspect.isPending} disabled={!message.trim() || !targets.length || reselect.isPending || confirm.isPending} onClick={() => inspect.mutate()}>预览逐群执行计划</Button>
      {dirty && <Alert type="warning" title="文字或目标群已修改，请重新预览" />}
      {preview && !dirty && <>
        {resourcesChanged&&<Alert type="warning" title="业务资源已变化，请重新预览后确认" description={<Button onClick={()=>inspect.mutate()}>重新预览</Button>}/>}
        {preview.created_at&&<p>受理预览时间：{new Date(preview.created_at).toLocaleString()}</p>}
        {liveOverview?.readiness.some(live=>{const old=preview.plan.items.flatMap(i=>i.readiness??[]).find(r=>r.binding_id===live.binding_id);return old && (old.state!==live.state || old.ready!==live.ready)}) && <Alert type="warning" title="状态已更新；当前发送计划保持原快照" description={<Button onClick={()=>inspect.mutate()}>更新业务预览</Button>}/>}
        {preview.plan.sending_status&&!preview.plan.sending_status.can_confirm&&<Alert type="warning" title={preview.plan.sending_status.message}/>}
        <p>当前为固定计划；业务范围变化后请重新点击“预览逐群执行计划”，并重新确认。计划有效期：{preview.expires_at}。以下为实际发送文字：</p><pre style={{ whiteSpace: 'pre-wrap' }}>{preview.plan.message}</pre>
        <Table rowKey="target_id" pagination={false} dataSource={preview.plan.items} columns={[
          { title: '目标群', render:(_,i)=>i.target.name }, { title: '首选手机', render: (_, i) => <Select aria-label={`目标群 ${i.target_id} 的共享手机`} style={{minWidth:150}} disabled={reselect.isPending} value={i.selected?.device_id} placeholder={i.readiness?.length ? '选择已绑定手机（可离线）' : '无绑定设备'} options={(i.readiness ?? []).map(r=>({value:r.device_id,label:r.device_name,disabled:!i.candidates.some(c=>c.device_id===r.device_id) || r.plan_selectable===false}))} onChange={device_id=>reselect.mutate(previewSelections(preview.plan.items,i.target_id,device_id))} /> },
          { title: '实际搜索文字', render: (_, i) => i.selected?.search_text ?? i.target.search_text },
          { title: '受理检查与手机执行状态', render: (_, i) => <Space orientation="vertical">
            <p>{i.blocker ? `受理检查：${i.blocker}` : '可以确认提交；手机执行条件不影响任务入队'}</p>
            {(i.readiness ?? []).filter(r=>!i.selected||r.device_id===i.selected.device_id).map(snapshot=>{
              const current=currentReadiness(snapshot,liveOverview,synchronized)
              return <div key={snapshot.binding_id}>
                <SharedReadinessView snapshot={snapshot} current={current} blocked={!!i.blocker}/>
                {preview.plan.dispatch_mode==='candidate_pool'&&<p>确认的授权候选：{i.candidates.filter(c=>c.confirmable!==false).map(c=>`${c.device_name}（${c.search_text}）`).join('、')}；所选为首选，范围内允许自动换机。</p>}
                {admin&&snapshot.blockers.some(b=>isUsageBlocker(b.code))&&<Button onClick={()=>setUsageOpen(true)}>查看本校使用范围</Button>}
              </div>
            })}
            {i.blocker&&!(i.readiness?.length)&&<p>没有可选绑定，请核对本校目标群与手机的关联</p>}
          </Space> },
        ]} />
        {confirmationIssues.length > 0 && <Alert type="warning" title={admin ? "提交时需要核对以下问题" : "没有本校管理员发送权限"} description={admin ? `${confirmationIssues.length} 个目标需要处理受理问题，请查看上表。` : confirmationIssues[0]}/>}
        <Button type="primary" loading={confirm.isPending} disabled={!admin || resourcesChanged || reselect.isPending || inspect.isPending} onClick={() => confirm.mutate()}>确认发送</Button>
      </>}
      <Table<SharedTask> rowKey="id" loading={overview.isFetching} dataSource={overview.data?.tasks ?? []} pagination={{ pageSize: 10 }} columns={[
        { title: '任务', dataIndex: 'id' }, { title: '创建时间', dataIndex: 'created_at' }, { title: '状态', render: (_, t) => t.stopped ? '已停止领取' : '查看逐群进度' },
        { title: '操作', render: (_, t) => <Button onClick={() => setSelected({school,id:t.id})}>查看计划与结果</Button> },
      ]} />
    </Space>
    <Modal title="手机群消息使用管理" open={usageOpen} footer={null} width={1000} onCancel={()=>setUsageOpen(false)}><UsageManagement school={school} admin={admin}/></Modal>
    <Modal title="共享任务逐群进度" open={!!selected && selected.school===school} footer={null} onCancel={() => setDetail(undefined)}>
      {detailQuery.isPending && <p>正在读取逐群进度…</p>}
      {!!detailQuery.dataUpdatedAt && <p>最近获取时间：{new Date(detailQuery.dataUpdatedAt).toLocaleString()}</p>}
      {detailQuery.error && <Alert type="warning" title="进度读取失败，以下为上次获取的快照" description={taskError(detailQuery.error)}/>}
      {detail && <p>确认有效至：{detail.confirmed_until ?? '待核对'}；{detail.dispatch_mode==='candidate_pool'?'目标独立调度，候选范围内自动换机':`历史任务当前顺序：${(detail.cursor_position??0)+1}`}；最早安排：{detail.next_eligible_at??'待核对'}。已到期的任务需要复制为新草稿并重新确认。</p>}
      {detail?.items?.length && <Button onClick={()=>{const items=[...detail.items!].sort((a,b)=>a.position-b.position);const plans=items.map(i=>i.attempts.find(a=>a.current)?.plan);const first=plans[0];if(!first)return;setDraft(undefined);setTargets(items.map(i=>i.target_id));setTargetNames(Object.fromEntries(plans.filter(p=>!!p).map(p=>[p.target_id,p.target_name??p.search_text])));setMessage(first.message);setGap(first.between_targets_seconds);setPreview(undefined);setDetail(undefined)}}>复制原意图为新草稿（需重新确认）</Button>}
      {detail && admin && !detail.stopped && <Button danger onClick={()=>setAction({kind:'stop'})}>停止尚未发放许可的任务</Button>}
      {detail?.items?.map(i => <div key={i.id}><p>目标群：{i.attempts.find(a=>a.current)?.plan.target_name || i.attempts[0]?.plan.search_text || '目标群已移除'}</p>{i.dispatch&&<div><p>目标调度：{dispatchLabel(i.dispatch.state)}</p><p>授权候选：{i.dispatch.candidates.map(c=>`${c.device_name}（${c.search_text}）`).join('、')}</p>{i.dispatch.assignments.map(r=><p key={r.id}>{r.source==='automatic'?'自动换机':'人工改派'}：{r.from_device_name??'原手机'} → {r.to_device_name??'候选手机'}（{new Date(r.created_at).toLocaleString()}）</p>)}</div>}{i.attempts?.map(a => <p key={a.id}>{a.plan.search_text} / {a.plan.device_name || a.plan.device_id} / {a.state==='queued'?(i.dispatch?'平台已受理，按授权候选调度':a.device_received_at?'手机已接收，等待执行条件':'平台已创建，等待手机同步'):stateLabels[a.state]}{a.result_reason&&['failed_before_call','unknown'].includes(a.state)&&<span>；本次尝试：{attemptFailureReason(a.result_reason)}</span>}{a.device_received_at&&<small>（手机接收：{new Date(a.device_received_at).toLocaleString()}）</small>}{a.execution_status?.conditions.map(c=><span key={c.code}>；{c.message}（{c.owner==='system'?'系统自动处理':c.owner==='device'?'手机端处理':'管理员处理'}）{c.waiting_since&&<small> 等待起点：{new Date(c.waiting_since).toLocaleString()}</small>}{c.reason_changed_at&&<small> 最近原因变化：{new Date(c.reason_changed_at).toLocaleString()}</small>}{c.retry_at && <small> 最早重新判断：{new Date(c.retry_at).toLocaleString()}</small>}</span>)}{admin && a.state==='unknown' && <Button onClick={()=>setAction({kind:'review',attempt:a})}>人工核对结果</Button>}{admin && a.current && ['failed_before_call','verified_not_sent','invalidated'].includes(a.state) && <Button onClick={()=>setAction({kind:'reassign',attempt:a})}>核对后重新安排设备</Button>}</p>)}</div>)}
    </Modal>
    <Modal title={action?.kind==='stop'?'停止新领取':action?.kind==='review'?'人工核对原发送结果':'重新安排原目标群'} open={!!action} confirmLoading={mutate.isPending} onCancel={()=>setAction(undefined)} okButtonProps={{disabled:!reason.trim() || (action?.kind==='reassign'&&!device)}} onOk={()=>mutate.mutate()}>
      <p>此操作保留原计划和全部尝试记录。停止不会撤回已发出的调用许可；确定未发送时可在授权候选内改派，无需等待旧设备恢复；新设备实际调用仍检查自身状态与授权。</p>
      {action?.kind==='review' && <Select value={decision} onChange={setDecision} style={{width:'100%'}} options={[{value:'still_unknown',label:'仍然无法确定，继续阻断'},{value:'verified_not_sent',label:'已有充分依据确认未发送'},{value:'manually_confirmed_sent',label:'人工核对确认已发送'}]} />}
      {action?.kind==='reassign' && <Select aria-label="重新安排共享手机" value={device} onChange={setDevice} style={{width:'100%'}} options={(bindings.data?.items??[]).filter(b=>b.enabled && b.target_id===action.attempt?.plan.target_id).map(b=>({value:b.device_id,label:b.device_name||b.device_id}))} />}
      <Input.TextArea aria-label="核对依据与操作理由" value={reason} onChange={e=>setReason(e.target.value)} maxLength={2000} placeholder="记录实际核对依据，不能仅因等待超时判断未发送" />
      {mutate.error && <Alert type="error" title={taskError(mutate.error)} />}
    </Modal>
  </Card>
}

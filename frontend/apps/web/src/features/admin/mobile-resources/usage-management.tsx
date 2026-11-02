import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Checkbox, Input, InputNumber, Modal, Select, Space, Table, Tag } from 'antd'
import { sharedSend, type UsageInput, type UsagePreview, type MobileUsage, type UsageOperation } from './api'
import { BusinessSchemeEditor } from './business-scheme'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

export function UsageManagement({school,admin=true}:{school?:string;admin?:boolean}) {
 const cache=useQueryClient()
 const keys=useRef(new Map<string,string>())
 function key(kind:string,body:unknown){const id=JSON.stringify([kind,body]);let v=keys.current.get(id);if(!v){v=createIdempotencyKey();keys.current.set(id,v)}return v}
 const [selectedSchool,setSchool]=useState(school)
 const [open,setOpen]=useState(false)
 const [devices,setDevices]=useState<string[]>([])
 const [targets,setTargets]=useState<string[]>([])
 const [newGroup,setNewGroup]=useState('')
 const [limit,setLimit]=useState(100)
 const [expiry,setExpiry]=useState('')
 const [auto,setAuto]=useState(false)
 const [renewal,setRenewal]=useState<MobileUsage>()
 const [preview,setPreview]=useState<UsagePreview>()
 const [action,setAction]=useState<{row:MobileUsage;operation:UsageOperation|'issue'|'handle_issue';issue?:{id:string;revision:number}}>()
 const [issueState,setIssueState]=useState<'investigating'|'resolved'|'declined'>('investigating')
 const [reason,setReason]=useState('')
 const list=useQuery({queryKey:['mobile-execution',school??'platform','usages'],queryFn:()=>sharedSend.usages(school),refetchInterval:20000})
 const globalOptions=useQuery({queryKey:['mobile-execution',school??'platform','usage-options'],queryFn:()=>sharedSend.usageOptions(school)})
 const options=useQuery({queryKey:['mobile-execution',selectedSchool,'usage-options'],queryFn:()=>sharedSend.usageOptions(selectedSchool!),enabled:!!selectedSchool})
 const refresh=()=>cache.invalidateQueries({queryKey:['mobile-execution']})
 const inspect=useMutation({mutationFn:()=>{
  const input:UsageInput={device_ids:devices,targets:[...(options.data?.targets.filter(t=>targets.includes(t.id)).map(t=>({id:t.id,name:t.name,search_text:t.search_text}))??[]),...(newGroup.trim()?[{id:null,name:newGroup,search_text:newGroup}]:[])],max_calls:limit,expires_at:new Date(expiry).toISOString(),auto_targets:auto,...(renewal?{renewal_of:renewal.id,expected_revision:renewal.revision}:{})}
  return sharedSend.usagePreview(selectedSchool!,input,key('preview',[selectedSchool,input]))
 },onSuccess:setPreview})
 const confirm=useMutation({mutationFn:()=>sharedSend.usageConfirm(selectedSchool!,{preview_id:preview!.id,digest:preview!.digest},key('confirm',preview!.id)),onSuccess:()=>{setOpen(false);setPreview(undefined);void refresh()}})
 const mutate=useMutation<unknown,Error>({mutationFn:()=>action!.operation==='handle_issue'?sharedSend.usageIssueUpdate(action!.row.school_id,action!.row.id,action!.issue!.id,{expected_revision:action!.issue!.revision,state:issueState,response:reason},key('issue-response',[action,issueState,reason])):action!.operation==='issue'?sharedSend.usageIssue(action!.row.school_id,action!.row.id,reason,key('issue',[action!.row.id,reason])):sharedSend.usageAction(action!.row.school_id,action!.row.id,action!.operation,{expected_revision:action!.row.revision,reason},key('action',[action,reason])),onSuccess:()=>{setAction(undefined);setReason('');void refresh()}})
 function begin(row?:MobileUsage){setRenewal(row?.approval==='approved'?row:undefined);setSchool(row?.school_id??school);setDevices(row?.body.device_ids??[]);setTargets(row?.body.targets?.map(t=>t.id!).filter(Boolean)??[]);setNewGroup('');setLimit(row?.body.max_calls??100);setExpiry(new Date(Date.now()+30*86400000).toISOString().slice(0,16));setAuto(row?.body.auto_targets??false);setPreview(undefined);inspect.reset();confirm.reset();setOpen(true)}
 function operate(row:MobileUsage,operation:UsageOperation|'issue'){setAction({row,operation});setReason('');mutate.reset()}
 const error=list.error||globalOptions.error||options.error
 return <Card title="手机群消息使用管理" extra={admin&&<Button type="primary" onClick={()=>begin()}>开通使用</Button>}>
  <Alert type="info" showIcon title="选范围，核对后确认" description="平台确认后直接开通；学校在已批准范围内可直接使用，新增范围提交平台审批。离线手机也可选择，恢复在线后自动检查。使用范围与手机当前状态分别管理，已批准范围会保留，手机就绪后自动继续。"/>
  {globalOptions.data?.sending_status&&<Alert type={globalOptions.data.sending_status.can_confirm?'success':'warning'} showIcon title={globalOptions.data.sending_status.message} description={`实际执行：${({default:'系统默认',unconfigured:'未配置',disabled:'已关闭',draft_only:'仅内容准备',preview_only:'仅预览',unsupported:'方案需更新',enabled:'已开放'} as const)[globalOptions.data.sending_status.platform]}。使用审批与手机现场状态见下表。`}/>}
  {globalOptions.data?.scheme&&<p>当前业务方案：时区 {globalOptions.data.scheme.timezone}；每所学校每日最多 {globalOptions.data.scheme.daily_max_calls} 次；每个目标群最小间隔 {globalOptions.data.scheme.interval_seconds} 秒。</p>}
  {!school&&admin&&<BusinessSchemeEditor scheme={globalOptions.data?.business_scheme} status={globalOptions.data?.sending_status}/>}
  {error&&<Alert type="error" title={error.message}/>}
  <Table<MobileUsage> rowKey="id" dataSource={list.data?.items??[]} loading={list.isFetching} pagination={{pageSize:10}} columns={[
   {title:'学校',dataIndex:'school_name'},
   {title:'使用范围',render:(_,u)=><><p>{u.body.targets.map(t=>t.name).join('、')||'指定目标群'} / {u.body.device_ids.length} 台手机</p><p>累计最多 {u.body.max_calls} 次；有效至 {new Date(u.expires_at).toLocaleString()}</p>{u.body.auto_targets&&<Tag>包含本校后续新增群</Tag>}</>},
   {title:'审批',render:(_,u)=>({pending:'待平台审批',approved:'已批准',declined:'已退回'})[u.approval]},
   {title:'使用状态',render:(_,u)=>new Date(u.expires_at)<=new Date()?'已到期':u.state==='paused'?'已暂停':u.state==='revoked'?'已撤销':u.approval!=='approved'?'等待审批':u.delivery==='available'?'已开通，现场条件自动检查':u.progress},
   {title:'操作',render:(_,u)=><Space wrap>{admin&&<>
    {!school&&u.approval==='pending'&&<><Button onClick={()=>operate(u,'approve')}>批准</Button><Button onClick={()=>operate(u,'decline')}>退回</Button></>}
    {u.state!=='revoked'&&<><Button onClick={()=>begin(u)}>续期或重新申请</Button><Button onClick={()=>operate(u,u.state==='paused'?'resume':'pause')}>{u.state==='paused'?'恢复':'暂停'}</Button><Button danger onClick={()=>operate(u,'revoke')}>撤销</Button></>}
    <Button onClick={()=>operate(u,'issue')}>报告问题</Button></>}</Space>},
  ]} expandable={{expandedRowRender:u=><><p>{u.progress}</p>{u.readiness?.map((r,i)=><p key={i}>{r.target_name} / {r.device_name}：{r.ready?'可执行':r.messages.join('；')}</p>)}{u.issues.map(i=><p key={i.id}>问题：{i.description}（{({submitted:'已提交',investigating:'处理中',resolved:'已答复',declined:'不受理'} as Record<string,string>)[i.state]??i.state}）{i.response&&<span>；平台答复：{i.response}</span>}{!school&&admin&&<Button onClick={()=>{setAction({row:u,operation:'handle_issue',issue:i});setReason(i.response);setIssueState('investigating');mutate.reset()}}>处理问题</Button>}</p>)}{u.events.map(e=><p key={e.id}>{new Date(e.created_at).toLocaleString()} · {({approved:'批准开通',pending:'提交申请',approve:'平台批准',decline:'平台退回',pause:'暂停',resume:'恢复',revoke:'撤销',reused:'使用原已批准范围',issue_response:'平台处理问题'} as Record<string,string>)[e.action]??'使用记录'}</p>)}</>}}/>
  <Modal title={preview?'核对开通摘要':'选择学校、手机和目标群'} open={open} onCancel={()=>setOpen(false)} footer={null} destroyOnHidden>
   <Space orientation="vertical" style={{width:'100%'}}>
   {!preview?<>
    {!school&&<Select aria-label="使用学校" placeholder="选择学校" style={{width:'100%'}} value={selectedSchool} onChange={s=>{setSchool(s);setTargets([]);setDevices([])}} options={globalOptions.data?.schools.map(s=>({value:s.id,label:s.name}))}/>}
    <Select aria-label="候选手机" mode="multiple" placeholder="选择手机（可离线）" style={{width:'100%'}} value={devices} onChange={setDevices} options={options.data?.devices.map(d=>({value:d.id,label:`${d.name}（${d.online?'在线':'离线，恢复后执行'}）`}))}/>
    <Select aria-label="目标群" mode="multiple" placeholder="选择已有群" style={{width:'100%'}} value={targets} onChange={setTargets} options={options.data?.targets.map(t=>({value:t.id,label:t.name}))}/>
    <Input aria-label="同页新增群" placeholder="或直接填写新增群的实际搜索名" value={newGroup} maxLength={256} onChange={e=>setNewGroup(e.target.value)}/>
    <label>累计调用额度 <InputNumber aria-label="累计调用额度" value={limit} min={1} max={100000} precision={0} onChange={v=>setLimit(v??1)}/></label>
    <label>有效期 <Input aria-label="使用有效期" type="datetime-local" value={expiry} onChange={e=>setExpiry(e.target.value)}/></label>
    {!school&&<Checkbox checked={auto} onChange={e=>setAuto(e.target.checked)}>明确批准本校后续新增群</Checkbox>}
    <p>使用方案：{options.data?.scheme?`每日最多 ${options.data.scheme.daily_max_calls} 次，目标群间隔至少 ${options.data.scheme.interval_seconds} 秒；时间按 ${options.data.scheme.timezone}。`:'等待平台管理员配置业务方案，申请可先保存。'}</p>
    <Button type="primary" loading={inspect.isPending} disabled={!selectedSchool||!devices.length||(!targets.length&&!newGroup.trim())||!expiry} onClick={()=>inspect.mutate()}>下一步：核对摘要</Button>
   </>:<>
    <p>学校：{globalOptions.data?.schools.find(s=>s.id===selectedSchool)?.name??'本校'}</p>
    <p>手机：{preview.body.device_ids.map(id=>options.data?.devices.find(d=>d.id===id)?.name??'已选择手机').join('、')}</p>
    <p>群：{preview.body.targets.map(t=>t.name).join('、')}。{preview.body.auto_targets?'包含本校后续新增群。':''}</p>
    <p>累计额度：{preview.body.max_calls} 次；有效至 {new Date(preview.body.expires_at).toLocaleString()}。</p>
    <p>{school?'在原批准范围内直接使用，其余范围提交平台审批。':'确认即批准此使用范围。'}使用范围会保留，手机恢复可用后自动继续。</p>
    {preview.body.scheme&&<p>每日上限：{preview.body.scheme.daily_max_calls} 次；最小间隔：{preview.body.scheme.interval_seconds} 秒。允许时段：{preview.body.scheme.allowed_windows.map(w=>`周 ${w.weekdays.join('、')} ${Math.floor(w.start_minute/60)}:${String(w.start_minute%60).padStart(2,'0')}–${Math.floor(w.end_minute/60)}:${String(w.end_minute%60).padStart(2,'0')}`).join('；')}（{preview.body.scheme.timezone}）</p>}
    <Space><Button onClick={()=>setPreview(undefined)}>返回修改</Button><Button type="primary" loading={confirm.isPending} onClick={()=>confirm.mutate()}>确认{school?'开通或提交申请':'开通'}</Button></Space>
   </>}
   {(inspect.error||confirm.error)&&<Alert type="error" title={(inspect.error||confirm.error)?.message}/>}
   </Space>
  </Modal>
  <Modal title={action?.operation==='issue'?'报告使用问题':'确认处理使用授权'} open={!!action} onCancel={()=>setAction(undefined)} onOk={()=>mutate.mutate()} confirmLoading={mutate.isPending} okButtonProps={{disabled:!reason.trim()}}>
   <p>{action?.operation==='issue'?'系统自动关联已有检查和使用记录，请说明遇到的问题。':'操作只影响这份授权的新领取与新许可，已发调用继续保留结果和核对记录。'}</p>
   {action?.operation==='handle_issue'&&<Select aria-label="问题处理结果" value={issueState} onChange={setIssueState} options={[{value:'investigating',label:'处理中'},{value:'resolved',label:'已答复'},{value:'declined',label:'不受理'}]}/>}
   <Input.TextArea aria-label="处理说明" value={reason} onChange={e=>setReason(e.target.value)} maxLength={2000}/>
   {mutate.error&&<Alert type="error" title={mutate.error.message}/>}
  </Modal>
 </Card>
}

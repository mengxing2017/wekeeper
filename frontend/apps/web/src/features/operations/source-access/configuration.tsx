import {VersionPolicyEditor} from './automation'
import {useEffect,useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Checkbox,Descriptions,Drawer,Form,Input,Modal,Popconfirm,Select,Space,Table,Tag,Typography} from 'antd'
import {sourceRouting,routingIssueLabels,type RouteConfiguration,type RouteConfigurationInput,type RoutingAction,type RoutingChannel,type RoutingHandoff,type RoutingCheck} from './api'

export function RouteConfigurationEditor({route,onChanged,manageAuthorizations=true}:{route:string;onChanged:()=>Promise<void>;manageAuthorizations?:boolean}) {
 const [form]=Form.useForm<RouteConfigurationInput>()
 const [base,setBase]=useState<RouteConfiguration|null>(null),[conflict,setConflict]=useState<RouteConfiguration|null>(null)
 const [error,setError]=useState(''),[busy,setBusy]=useState(false),[check,setCheck]=useState<RoutingCheck|null>(null)
 const [action,setAction]=useState<RoutingAction|null>(null),[reason,setReason]=useState(''),[username,setUsername]=useState('')
 const [policyOpen,setPolicyOpen]=useState(false)
 const [canRecheck,setCanRecheck]=useState(false)
 const [handoff,setHandoff]=useState<RoutingHandoff|null>(null),[historyPage,setHistoryPage]=useState(1)
 const q=useQuery({queryKey:['route-configuration',route],queryFn:()=>sourceRouting.configuration(route)})
 const units=useQuery({queryKey:['routing-units'],queryFn:()=>sourceRouting.entities('routing-units')})
 const groups=useQuery({queryKey:['routing-groups'],queryFn:()=>sourceRouting.entities('routing-groups')})
 const history=useQuery({queryKey:['route-revisions',route,historyPage],queryFn:()=>sourceRouting.revisions(route,historyPage)})
 const unit=Form.useWatch('unit_id',form)
 const channel=Form.useWatch(['execution','channel'],form) as RoutingChannel|undefined
 const workspace=Form.useWatch(['execution','workspace_id'],form) as string|undefined
 const spaces=useQuery({queryKey:['route-workspaces',channel],queryFn:()=>sourceRouting.workspaces(channel!),enabled:!!channel})
 const targets=useQuery({queryKey:['route-targets',channel,workspace],queryFn:()=>sourceRouting.targets(channel!,workspace!),enabled:!!channel&&!!workspace})
 const apply=(c:RouteConfiguration)=>{
  setBase(c);setConflict(null);form.resetFields();form.setFieldsValue({unit_id:c.unit_id??undefined,group_id:c.group_id??undefined,execution:c.snapshot?.execution??undefined})
 }
 useEffect(()=>{if(q.data&&!base){setBase(q.data);form.setFieldsValue({unit_id:q.data.unit_id??undefined,group_id:q.data.group_id??undefined,execution:q.data.snapshot?.execution??undefined})}},[q.data,base,form])
 const refresh=async()=>{const c=await sourceRouting.configuration(route);apply(c);await Promise.all([q.refetch(),history.refetch(),onChanged()])}
 const perform=async(work:()=>Promise<unknown>)=>{
  setBusy(true);setError('')
  try{await work();await refresh()}
  catch(e){setError((e as Error).message);const latest=await sourceRouting.configuration(route).catch(()=>null);if(latest&&base&&latest.revision!==base.revision)setConflict(latest)}
  finally{setBusy(false)}
 }
 const save=()=>perform(async()=>{
  const v=await form.validateFields()
  await sourceRouting.configure(route,{revision:base!.revision,unit_id:v.unit_id,group_id:v.group_id,execution:v.execution?.channel?{...v.execution,target_ids:v.execution.target_ids??[]}:null})
  setCheck(null)
 })
 const runCheck=async()=>{setError('');try{setCheck(await sourceRouting.check(route))}catch(e){setError((e as Error).message)}}
 const copyInfo=async()=>{setError('');try{setHandoff(await sourceRouting.handoff(route))}catch(e){setError((e as Error).message)}}
 const readonly=!!base?.retired_at
 return <Space orientation="vertical" style={{width:'100%'}}>
  {(error||q.error||units.error||groups.error||targets.error||spaces.error||history.error)&&<Alert type="error" title={error||q.error?.message||units.error?.message||groups.error?.message||targets.error?.message||spaces.error?.message||history.error?.message}/>}
  <Descriptions column={1} items={[
   {key:'route',label:'消息路由 ID',children:<Typography.Text copyable>{route}</Typography.Text>},
   {key:'status',label:'接收状态',children:base?.status==='active'?'启用':'停用'},
   {key:'execution',label:'自动运行状态',children:readonly?'已退役':({draft:'配置草稿',running:'运行中',paused:'已暂停'}[base?.execution_state??'draft'])},
   {key:'revision',label:'配置修订',children:base?.config_revision??0},
  ]}/>
  {conflict&&<Alert type="warning" title="配置已被其他人修改，本地输入已保留" description={<>
   <Descriptions column={1} items={[{key:'channel',label:'最新通道',children:conflict.snapshot?.execution?.channel??'未配置'},{key:'targets',label:'最新目标群',children:conflict.snapshot?.targets.map(t=>t.name).join('、')||'无'},{key:'rev',label:'最新修订',children:conflict.config_revision}]}/>
   <Popconfirm title="载入最新配置并替换当前未保存输入？" onConfirm={()=>apply(conflict)}><Button>载入最新配置</Button></Popconfirm>
  </>}/>}
  <Form form={form} layout="vertical" disabled={readonly||busy||!base}>
   <Form.Item name="unit_id" label="业务单位" rules={[{required:true,message:'请选择业务单位'}]}><Select disabled={!!base?.unit_id} onChange={()=>form.setFieldValue('group_id',undefined)} options={units.data?.items.map(u=>({value:u.id,label:`${u.name}${u.active?'':'（停用）'}`}))}/></Form.Item>
   <Form.Item name="group_id" label="受众分组" rules={[{required:true,message:'请选择受众分组'}]}><Select disabled={!!base?.group_id} options={groups.data?.items.filter(g=>g.unit_id===unit).map(g=>({value:g.id,label:`${g.name}${g.active?'':'（停用）'}`}))}/></Form.Item>
   <Form.Item name={['execution','channel']} label="发送通道"><Select allowClear onChange={()=>{form.setFieldValue(['execution','workspace_id'],undefined);form.setFieldValue(['execution','target_ids'],[])}} options={[{value:'wecom',label:'企业微信'},{value:'mobile',label:'移动端'}]}/></Form.Item>
   {channel&&<>
    <Form.Item name={['execution','workspace_id']} label="通道工作空间" rules={[{required:true,message:'请选择通道工作空间'}]}><Select onChange={()=>form.setFieldValue(['execution','target_ids'],[])} options={spaces.data?.items.map(w=>({value:w.id,label:w.name}))}/></Form.Item>
    <Form.Item name={['execution','target_ids']} label="目标群"><Select mode="multiple" optionFilterProp="label" options={targets.data?.items.map(t=>({value:t.id,label:t.name}))}/></Form.Item>
    <Typography.Paragraph type="secondary">仅投递到已选择的群；手机离线不影响保存和平台受理。</Typography.Paragraph>
   </>}
   <Button type="primary" loading={busy} disabled={!!conflict} onClick={save}>保存执行配置</Button>
  </Form>
  <Button disabled={readonly} onClick={()=>setPolicyOpen(true)}>版本投递策略</Button>
  <Drawer open={policyOpen} title="版本投递策略" onClose={()=>setPolicyOpen(false)}>{policyOpen&&<VersionPolicyEditor route={route}/>}</Drawer>
  <Space wrap>
   <Button disabled={!base||busy} onClick={runCheck}>检查启用条件</Button>
   <Button disabled={!base||busy||readonly} onClick={()=>{setReason('');setAction(base?.execution_state==='paused'?'resume':'enable')}}>{base?.execution_state==='paused'?'恢复自动执行':'启用自动执行'}</Button>
   <Button disabled={!base||busy||readonly||base.execution_state!=='running'} onClick={()=>{setReason('');setAction('pause')}}>暂停自动执行</Button>
   <Button danger disabled={!base||busy||readonly} onClick={()=>{setReason('');setAction('retire')}}>退役路由</Button>
   {manageAuthorizations&&<Button onClick={copyInfo}>生成接入说明</Button>}
  </Space>
  {check&&<Alert type={check.ready?'success':'warning'} title={check.ready?'业务配置完整，可启用':check.issues.map(i=>routingIssueLabels[i]??i).join('；')}/>}
  {manageAuthorizations&&<>
  <Typography.Title level={5}>路由配置管理员</Typography.Title>
  <Space><Input aria-label="路由配置管理员用户名" value={username} onChange={e=>setUsername(e.target.value)}/><Button disabled={!username.trim()||busy||!base||readonly} onClick={()=>perform(()=>sourceRouting.manager(route,base!.revision,username,true,canRecheck))}>授权配置管理</Button><Checkbox checked={canRecheck} onChange={e=>setCanRecheck(e.target.checked)}>允许重检收件</Checkbox></Space>
  <Table rowKey="id" dataSource={base?.managers??[]} pagination={false} columns={[{title:'用户名',dataIndex:'username'},{title:'收件重检',render:(_,m)=>m.can_recheck?'允许':'未授权'},{title:'操作',render:(_,m)=><Popconfirm title="撤销此路由配置管理授权？" onConfirm={()=>perform(()=>sourceRouting.manager(route,base!.revision,m.username,false))}><Button disabled={busy||readonly}>撤销配置管理</Button></Popconfirm>}]}/>
  </>}
  <Typography.Title level={5}>配置历史</Typography.Title>
  <Table rowKey="revision" dataSource={history.data?.items??[]} pagination={false} columns={[{title:'修订',dataIndex:'revision'},{title:'通道',render:(_,r)=>r.snapshot.execution?.channel??'未配置'},{title:'目标群',render:(_,r)=>r.snapshot.targets.map(t=>t.name).join('、')},{title:'保存时间',dataIndex:'created_at'}]}/>
  <Space><Button disabled={historyPage===1} onClick={()=>setHistoryPage(historyPage-1)}>上一版</Button><Tag>{historyPage}</Tag><Button disabled={!history.data?.has_more} onClick={()=>setHistoryPage(historyPage+1)}>更早版本</Button></Space>
  <Modal okText="确认变更" cancelText="取消" open={action!==null} title={action==='retire'?'退役后不能恢复或重新分配标识符':'变更自动运行状态'} onCancel={()=>setAction(null)} onOk={()=>perform(async()=>{if(!reason.trim())throw new Error('请填写操作原因');await sourceRouting.lifecycle(route,base!.revision,action!,reason);setAction(null)})} confirmLoading={busy}>
   <Typography.Paragraph>{action==='enable'||action==='resume'?'启用即授权本路由按已保存的通道、工作空间和目标自动分发后续来源消息，无需逐条确认。':'此操作仅控制平台尚未提交的发送。'}已交付的执行包或已提交的官方请求仍会按实际结果入账。</Typography.Paragraph>
   <Input.TextArea aria-label="状态变更原因" value={reason} onChange={e=>setReason(e.target.value)} maxLength={2000}/>
  </Modal>
  <Modal open={handoff!==null} title="接入说明" onCancel={()=>setHandoff(null)} footer={<Button onClick={()=>setHandoff(null)}>关闭</Button>}>
   {handoff&&<Typography.Paragraph copyable>{`接收端点：${window.location.origin}${handoff.receiver_path}\nroute_id：${handoff.route_id}\n认证：${handoff.authentication}，版本 ${handoff.signature_versions.join('、')}\n内容版本：${handoff.schema_versions.join('、')}\n契约：${handoff.contract_version}；配置修订：${handoff.config_revision}\nkey_id 与 secret 请通过受控凭据交付取得。`}</Typography.Paragraph>}
  </Modal>
 </Space>
}

import {BusinessSchemePage} from '../../admin/mobile-resources/business-scheme'
import { TableText } from '../../../shared/table-text'
import {useState} from 'react'
import {useMutation,useQuery,useQueryClient} from '@tanstack/react-query'
import {Alert,Button,Card,Form,Input,Modal,Select,Space,Switch,Table,Tag} from 'antd'
import {PaginationControls} from '../../../shared/pagination-controls'
import {receivingConfigurations,receivingUnits,type ReceivingChannel,type ReceivingConfiguration,type ReceivingConfigurationInput} from './api'
export function ReceivingConfigurations({channel,scope,admin,platformAdmin}:{channel:ReceivingChannel;scope?:string;admin:boolean;platformAdmin:boolean}){
 const cache=useQueryClient(),client=receivingConfigurations(channel)
 const [selected,setSelected]=useState(''),[page,setPage]=useState(1),[editor,setEditor]=useState<string>()
 const unit=scope||selected
 const units=useQuery({queryKey:['receiving-business-units'],queryFn:receivingUnits,enabled:!scope&&platformAdmin})
 const [form]=Form.useForm<ReceivingConfigurationInput>()
 const [targetName,setTargetName]=useState(''),[directory,setDirectory]=useState<string>()
 const targetKind=Form.useWatch('client_kind',form)
 const key=['receiving-configurations',channel,unit]
 const query=useQuery({queryKey:[...key,page],queryFn:()=>client.list(unit,page),enabled:!!unit})
 const resources=useQuery({queryKey:['receiving-resources',channel,unit],queryFn:()=>client.resources(unit),enabled:!!unit&&admin})
 const save=useMutation({mutationFn:(v:ReceivingConfigurationInput)=>client.save(unit,editor!,{...v,client_kind:channel==='mobile'?v.client_kind:undefined,device_ids:channel==='mobile'?v.device_ids:[],sender_ids:channel==='wecom'?v.sender_ids:[],device_dispatch_mode:channel==='mobile'?v.device_dispatch_mode:null}),onSuccess:async()=>{setEditor(undefined);await cache.invalidateQueries({queryKey:key})}})
 const target=useMutation({mutationFn:()=>client.target(unit,crypto.randomUUID(),{...(channel==='mobile'?{client_kind:targetKind}:{}),name:targetName,directory_group_id:channel==='wecom'?directory??null:null}),onSuccess:async row=>{await cache.invalidateQueries({queryKey:['receiving-resources',channel,unit]});form.setFieldValue('target_ids',[...(form.getFieldValue('target_ids')??[]),row.id]);setTargetName('');setDirectory(undefined)}})
 const edit=(row?:ReceivingConfiguration)=>{save.reset();setEditor(row?.id??crypto.randomUUID());form.resetFields();form.setFieldsValue({client_kind:row?.client_kind,name:row?.name??'',enabled:row?.enabled??false,target_ids:row?.target_ids??[],device_ids:row?.device_ids??[],sender_ids:row?.sender_ids??[],target_selection_mode:row?row.target_selection_mode:'all',device_dispatch_mode:channel==='mobile'?(row?row.device_dispatch_mode:'round_robin'):null})}
 return <Card title={channel==='mobile'?'手机接收配置':'企业微信接收配置'} extra={admin&&<Button type="primary" disabled={!unit} onClick={()=>edit()}>新增</Button>}>
 <Space orientation="vertical" style={{width:'100%'}}>
 {!scope&&<Select aria-label="业务单位" placeholder="选择业务单位" style={{minWidth:280}} value={selected||undefined} onChange={v=>{setSelected(v);setPage(1)}} options={units.data?.map(u=>({value:u.id,label:u.name}))}/>}
 <Alert type="info" title="按业务单位维护接收群与发送资源" description="启用即授权系统自动处理该单位之后的新消息。同一个群可供多个配置使用；每条消息按配置选择群；全部手机模式由每台手机分别发送，可能向同一群发送多次。配置变更不追溯已有消息。"/>
 {(query.error||resources.error||units.error)&&<Alert type="error" title={query.error?.message||resources.error?.message||units.error?.message}/>}
 {channel==='mobile'&&unit&&admin&&<BusinessSchemePage key={unit} unit={unit} unitName={units.data?.find(u=>u.id===unit)?.name??unit}/>}
 <Table<ReceivingConfiguration> tableLayout="fixed" scroll={{x:480}} rowKey="id" dataSource={query.data?.items??[]} loading={!!unit&&query.isFetching} pagination={false} columns={[{title:'名称',dataIndex:'name', render:value=><TableText value={value}/>},{title:'状态', width:80,render:(_,r)=><Tag>{r.enabled?'启用':'停用'}</Tag>},{title:'群接收方式',render:(_,r)=>r.target_selection_mode==='round_robin'?'轮询一个群':'全部群'},{title:'手机下发方式',render:(_,r)=>channel==='mobile'?(r.device_dispatch_mode==='all'?'全部手机分别执行':'轮询首选并自动换机'):'—'},{title:'群数量', width:96,render:(_,r)=>r.target_ids.length},...(admin?[{title:'操作', width:88,render:(_:unknown,r:ReceivingConfiguration)=><Button type="link" onClick={()=>edit(r)}>编辑</Button>}]:[])]}/>
 <PaginationControls page={page} more={query.data?.page?.has_more} busy={query.isFetching} change={setPage} subject="接收配置"/>
 </Space>
 <Modal title="接收配置" open={!!editor} onCancel={()=>setEditor(undefined)} onOk={()=>void form.validateFields().then(v=>save.mutate(v)).catch(()=>undefined)} okText="保存" cancelText="取消" confirmLoading={save.isPending}>
 {save.error&&<Alert type="error" title={save.error.message}/>}<Form form={form} layout="vertical">
 {channel==='mobile'&&<Form.Item name="client_kind" label="群类型" rules={[{required:true,message:'请选择群类型'}]}><Select onChange={()=>form.setFieldValue('target_ids',[])} options={[{value:'wecom',label:'企业微信'},{value:'wechat',label:'个人微信'}]}/></Form.Item>}
 <Form.Item name="name" label="名称" rules={[{required:true,whitespace:true,max:120}]}><Input/></Form.Item>
 <Form.Item name="enabled" label="授权自动发送新消息" valuePropName="checked"><Switch/></Form.Item>
 <Space orientation="vertical" style={{width:'100%'}}>
 <Input aria-label="新增群名称" placeholder="填写一个群名称" value={targetName} onChange={e=>setTargetName(e.target.value)}/>
 {channel==='wecom'&&<Select aria-label="已同步客户群" placeholder="选择已同步客户群" style={{width:'100%'}} value={directory} onChange={id=>{setDirectory(id);setTargetName(resources.data?.directory_groups?.find(g=>g.id===id)?.name??'')}} options={resources.data?.directory_groups?.map(g=>({value:g.id,label:g.name}))}/>}
 <Button disabled={!targetName.trim()||(channel==='mobile'&&!targetKind)||(channel==='wecom'&&!directory)} loading={target.isPending} onClick={()=>target.mutate()}>添加</Button>
 {target.error&&<Alert type="error" title={target.error.message}/>}
 </Space>
 <Form.Item name="target_selection_mode" label="群接收方式" rules={[{required:true}]}><Select options={[{value:'all',label:'全部群'},{value:'round_robin',label:'轮询一个群'}]}/></Form.Item>
 {channel==='mobile'&&<Form.Item name="device_dispatch_mode" label="手机下发方式" rules={[{required:true}]}><Select options={[{value:'round_robin',label:'轮询首选并自动换机'},{value:'all',label:'全部手机分别执行'}]}/></Form.Item>}
 <Form.Item noStyle shouldUpdate>{()=> <Alert type="info" title="自动发送授权摘要" description={`${form.getFieldValue('target_selection_mode')==='round_robin'?'每条新消息轮流选择一个群':'每条新消息发往全部所选群'}；${channel==='mobile'?(form.getFieldValue('device_dispatch_mode')==='all'?'全部所选手机各自发送并独立反馈，可能向同一群发送多次':'按消息轮流选择首选，失败在授权候选内自动换机'):'使用现有企业微信发送账号'}。配置顺序即轮询顺序，变更只影响新消息。`}/>}</Form.Item>
 <Form.Item name="target_ids" label="接收群（按选择顺序）"><Select mode="multiple" optionFilterProp="label" options={resources.data?.targets.filter(t=>channel!=='mobile'||t.client_kind===targetKind).map(t=>({value:t.id,label:t.name}))}/></Form.Item>
 <Form.Item noStyle shouldUpdate>{()=> <OrderedResources label="群顺序" ids={form.getFieldValue('target_ids')??[]} options={resources.data?.targets??[]} change={ids=>form.setFieldValue('target_ids',ids)}/>}</Form.Item>
 {channel==='mobile'?<Form.Item name="device_ids" label="候选手机（按选择顺序）"><Select mode="multiple" options={resources.data?.devices.map(t=>({value:t.id,label:t.name}))}/></Form.Item>:<Form.Item name="sender_ids" label="发送账号"><Select mode="multiple" options={resources.data?.senders.map(t=>({value:t.id,label:t.name}))}/></Form.Item>}
 <Form.Item noStyle shouldUpdate>{()=>channel==='mobile'&&<OrderedResources label="手机顺序" ids={form.getFieldValue('device_ids')??[]} options={resources.data?.devices??[]} change={ids=>form.setFieldValue('device_ids',ids)}/>}</Form.Item>
 {editor&&query.data?.items.find(r=>r.id===editor)&&<Alert type="info" title="下一轮起点（读取时参考）" description={(()=>{const row=query.data.items.find(r=>r.id===editor)!;return `群：${resources.data?.targets.find(t=>t.id===row.next_target_id)?.name??'列表首项'}；手机：${resources.data?.devices.find(d=>d.id===row.next_device_id)?.name??'列表首项'}。并发消息可能先占用位置。`})()}/>}
 </Form></Modal></Card>
}

function OrderedResources({label,ids,options,change}:{label:string;ids:string[];options:{id:string;name:string}[];change:(ids:string[])=>void}){
 const move=(index:number,offset:number)=>{const ordered=[...ids];[ordered[index],ordered[index+offset]]=[ordered[index+offset],ordered[index]];change(ordered)}
 return <div aria-label={label}>{ids.map((id,i)=><div key={id}><span>{i+1}. {options.find(r=>r.id===id)?.name??id}</span><Button size="small" aria-label={`${label}第${i+1}项上移`} disabled={i===0} onClick={()=>move(i,-1)}>上移</Button><Button size="small" aria-label={`${label}第${i+1}项下移`} disabled={i===ids.length-1} onClick={()=>move(i,1)}>下移</Button></div>)}</div>
}

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
 const key=['receiving-configurations',channel,unit]
 const query=useQuery({queryKey:[...key,page],queryFn:()=>client.list(unit,page),enabled:!!unit})
 const resources=useQuery({queryKey:['receiving-resources',channel,unit],queryFn:()=>client.resources(unit),enabled:!!unit&&admin})
 const save=useMutation({mutationFn:(v:ReceivingConfigurationInput)=>client.save(unit,editor!,{...v,device_ids:channel==='mobile'?v.device_ids:[],sender_ids:channel==='wecom'?v.sender_ids:[]}),onSuccess:async()=>{setEditor(undefined);await cache.invalidateQueries({queryKey:key})}})
 const target=useMutation({mutationFn:()=>client.target(unit,crypto.randomUUID(),{name:targetName,directory_group_id:channel==='wecom'?directory??null:null}),onSuccess:async row=>{await cache.invalidateQueries({queryKey:['receiving-resources',channel,unit]});form.setFieldValue('target_ids',[...(form.getFieldValue('target_ids')??[]),row.id]);setTargetName('');setDirectory(undefined)}})
 const edit=(row?:ReceivingConfiguration)=>{save.reset();setEditor(row?.id??crypto.randomUUID());form.setFieldsValue({name:row?.name??'',enabled:row?.enabled??false,target_ids:row?.target_ids??[],device_ids:row?.device_ids??[],sender_ids:row?.sender_ids??[]})}
 return <Card title={channel==='mobile'?'手机接收配置':'企业微信接收配置'} extra={admin&&<Button type="primary" disabled={!unit} onClick={()=>edit()}>新增</Button>}>
 <Space orientation="vertical" style={{width:'100%'}}>
 {!scope&&<Select aria-label="业务单位" placeholder="选择业务单位" style={{minWidth:280}} value={selected||undefined} onChange={v=>{setSelected(v);setPage(1)}} options={units.data?.map(u=>({value:u.id,label:u.name}))}/>}
 <Alert type="info" title="按业务单位维护接收群与发送资源" description="启用即授权系统自动处理该单位之后的新消息。同一个群可供多个配置使用；每条消息在同一通道只向该群投递一次。配置变更不追溯已有消息。"/>
 {(query.error||resources.error||units.error)&&<Alert type="error" title={query.error?.message||resources.error?.message||units.error?.message}/>}
 <Table<ReceivingConfiguration> tableLayout="fixed" scroll={{x:480}} rowKey="id" dataSource={query.data?.items??[]} loading={!!unit&&query.isFetching} pagination={false} columns={[{title:'名称',dataIndex:'name', render:value=><TableText value={value}/>},{title:'状态', width:80,render:(_,r)=><Tag>{r.enabled?'启用':'停用'}</Tag>},{title:'群数量', width:96,render:(_,r)=>r.target_ids.length},...(admin?[{title:'操作', width:88,render:(_:unknown,r:ReceivingConfiguration)=><Button type="link" onClick={()=>edit(r)}>编辑</Button>}]:[])]}/>
 <PaginationControls page={page} more={query.data?.page?.has_more} busy={query.isFetching} change={setPage} subject="接收配置"/>
 </Space>
 <Modal title="接收配置" open={!!editor} onCancel={()=>setEditor(undefined)} onOk={()=>void form.validateFields().then(v=>save.mutate(v))} okText="保存" cancelText="取消" confirmLoading={save.isPending}>
 {save.error&&<Alert type="error" title={save.error.message}/>}<Form form={form} layout="vertical">
 <Form.Item name="name" label="名称" rules={[{required:true,whitespace:true,max:120}]}><Input/></Form.Item>
 <Form.Item name="enabled" label="授权自动发送新消息" valuePropName="checked"><Switch/></Form.Item>
 <Space orientation="vertical" style={{width:'100%'}}>
 <Input aria-label="新增群名称" placeholder="填写一个群名称" value={targetName} onChange={e=>setTargetName(e.target.value)}/>
 {channel==='wecom'&&<Select aria-label="已同步客户群" placeholder="选择已同步客户群" style={{width:'100%'}} value={directory} onChange={id=>{setDirectory(id);setTargetName(resources.data?.directory_groups?.find(g=>g.id===id)?.name??'')}} options={resources.data?.directory_groups?.map(g=>({value:g.id,label:g.name}))}/>}
 <Button disabled={!targetName.trim()||(channel==='wecom'&&!directory)} loading={target.isPending} onClick={()=>target.mutate()}>添加</Button>
 {target.error&&<Alert type="error" title={target.error.message}/>}
 </Space>
 <Form.Item name="target_ids" label="接收群"><Select mode="multiple" optionFilterProp="label" options={resources.data?.targets.map(t=>({value:t.id,label:t.name}))}/></Form.Item>
 {channel==='mobile'?<Form.Item name="device_ids" label="候选手机（按选择顺序）"><Select mode="multiple" options={resources.data?.devices.map(t=>({value:t.id,label:t.name}))}/></Form.Item>:<Form.Item name="sender_ids" label="发送账号"><Select mode="multiple" options={resources.data?.senders.map(t=>({value:t.id,label:t.name}))}/></Form.Item>}
 </Form></Modal></Card>
}

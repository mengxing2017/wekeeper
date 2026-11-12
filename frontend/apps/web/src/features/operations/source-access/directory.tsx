import {AdManagers} from '../advertising'
import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Checkbox,Form,Input,Modal,Select,Space,Table,Tabs,Tag} from 'antd'
import {sourceRouting,type RoutingEntity,type RoutingEntityInput,type RoutingEntityKind} from './api'
export function RoutingDirectory(){
 const [adUnit,setAdUnit]=useState<string>()
 const [kind,setKind]=useState<RoutingEntityKind>('routing-units'),[editing,setEditing]=useState<RoutingEntity|null>(),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const [form]=Form.useForm<RoutingEntityInput>()
 const units=useQuery({queryKey:['routing-units'],queryFn:()=>sourceRouting.entities('routing-units')})
 const groups=useQuery({queryKey:['routing-groups'],queryFn:()=>sourceRouting.entities('routing-groups')})
 const q=kind==='routing-units'?units:groups
 const open=(row:RoutingEntity|null)=>{setError('');form.resetFields();form.setFieldsValue(row??{name:'',description:'',active:true});setEditing(row)}
 const save=async()=>{setBusy(true);setError('');try{const v=await form.validateFields();await sourceRouting.saveEntity(kind,{...v,...(editing?{revision:editing.revision}:{})},editing?.id);setEditing(undefined);await q.refetch()}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 return <>
  {(error||q.error)&&<Alert type="error" title={error||q.error?.message}/>}
  <Tabs activeKey={kind} onChange={v=>setKind(v as RoutingEntityKind)} items={[{key:'routing-units',label:'业务单位'},{key:'routing-groups',label:'受众分组'}]}/>
  <Button onClick={()=>open(null)}>新增{kind==='routing-units'?'单位':'分组'}</Button>
  <Table<RoutingEntity> rowKey="id" loading={q.isFetching} dataSource={q.data?.items??[]} pagination={{pageSize:10}} columns={[
   {title:'名称',dataIndex:'name'},{title:'标识',dataIndex:'id'},
   ...(kind==='routing-groups'?[{title:'所属单位',render:(_:unknown,r:RoutingEntity)=>units.data?.items.find(u=>u.id===r.unit_id)?.name??r.unit_id}]:[]),
   {title:'说明',dataIndex:'description'},{title:'状态',render:(_,r)=><Tag>{r.active?'启用':'停用'}</Tag>},
   {title:'操作',render:(_,r)=><Space><Button onClick={()=>open(r)}>编辑{kind==='routing-units'?'单位':'分组'}</Button>{kind==='routing-units'&&<Button onClick={()=>setAdUnit(r.id)}>广告管理授权</Button>}</Space>}
  ]}/>
  <Modal open={editing!==undefined} title={`${editing?'编辑':'新增'}${kind==='routing-units'?'单位':'分组'}`} onCancel={()=>setEditing(undefined)} onOk={save} confirmLoading={busy} okText="保存" cancelText="取消">
   <Space orientation="vertical" style={{width:'100%'}}>
    {error&&<Alert type="error" title={error}/>}
    <Form form={form} layout="vertical">
     <Form.Item name="name" label="名称" rules={[{required:true,whitespace:true,max:120}]}><Input maxLength={120}/></Form.Item>
     <Form.Item name="description" label="说明"><Input.TextArea maxLength={2000}/></Form.Item>
     {kind==='routing-groups'&&<Form.Item name="unit_id" label="所属业务单位" rules={[{required:true}]}><Select options={units.data?.items.map(u=>({value:u.id,label:u.name}))}/></Form.Item>}
     <Form.Item name="active" valuePropName="checked"><Checkbox>启用</Checkbox></Form.Item>
    </Form>
   </Space>
  </Modal>
 <Modal open={!!adUnit} title="广告管理授权" footer={null} destroyOnHidden onCancel={()=>setAdUnit(undefined)}>{adUnit&&<AdManagers key={adUnit} unit={adUnit}/>}</Modal>
 </>
}

import {RouteAdvertising} from '../advertising'
import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Drawer,Form,Input,Modal,Select,Space,Table,Tag} from 'antd'
import {SourceReceipts} from './receipts'
import {RouteConfigurationEditor} from './configuration'
import {sourceAccess,type Association,type SourceInput,type SourceRow} from './api'

export function CallerRoutes({caller,onChanged}:{caller:SourceRow;onChanged:()=>Promise<void>}) {
 const [receipts,setReceipts]=useState<string|null>(null)
 const [configuring,setConfiguring]=useState<string|null>(null)
 const [page,setPage]=useState(1),[search,setSearch]=useState(''),[status,setStatus]=useState('')
 const [editing,setEditing]=useState<SourceRow|null>(),[busy,setBusy]=useState(false),[error,setError]=useState('')
 const [form]=Form.useForm<SourceInput>()
 const q=useQuery({queryKey:['caller-routes',caller.id,page,search,status],queryFn:()=>sourceAccess.list('source-routes',page,search,status,caller.id)})
 const links=useQuery({queryKey:['caller-route-authorizations',caller.id],queryFn:async()=>{
  const items:Association[]=[]
  for(let p=1;;p++){const result=await sourceAccess.routes(caller.id,p);items.push(...result.items);if(!result.has_more)break}
  return items
 }})
 const refresh=async()=>{await Promise.all([q.refetch(),links.refetch(),onChanged()])}
 const associate=async(route:string,state:'active'|'revoked')=>{
  // Read the current key revision: creating and authorizing several routes changes it.
  for(let p=1;;p++){
   const result=await sourceAccess.list('source-credentials',p,'','',caller.id)
   const current=result.items.find(c=>c.id===caller.id)
   if(current){await sourceAccess.associate(caller.id,route,current.revision,state);return}
   if(!result.has_more)throw new Error('调用方凭据不存在，请刷新列表')
  }
 }
 const open=(row:SourceRow|null)=>{setError('');form.resetFields();form.setFieldsValue({business_unit_name:row?.business_unit_name,remark:row?.remark,status:row?.status??'active'});setEditing(row)}
 const save=async()=>{
  setBusy(true);setError('')
  try{
   const v=await form.validateFields()
   await sourceAccess.save('source-routes',{...v,key_id:caller.id,...(editing?{revision:editing.revision}:{})},editing?.id)
   setEditing(undefined)
  }catch(e){setError((e as Error).message??'请核对输入')}
  finally{await refresh();setBusy(false)}
 }
 const toggle=async(route:string,state:'active'|'revoked')=>{
  setBusy(true);setError('')
  try{await associate(route,state)}catch(e){setError((e as Error).message)}finally{await refresh();setBusy(false)}
 }
 return <>
  {(error||q.error||links.error)&&<Alert type="error" title={error||q.error?.message||links.error?.message}/>}
  <Space wrap>
   <Button onClick={()=>open(null)}>新增路由</Button>
   <Input.Search aria-label="搜索当前调用方路由" onSearch={value=>{setSearch(value);setPage(1)}}/>
   <Select aria-label="路由状态筛选" value={status} onChange={value=>{setStatus(value);setPage(1)}} options={[{value:'',label:'全部状态'},{value:'active',label:'启用'},{value:'disabled',label:'停用'}]}/>
  </Space>
  <Table<SourceRow> rowKey="id" loading={q.isFetching||links.isFetching} dataSource={q.data?.items??[]} pagination={false} columns={[
   {title:'单位名称',dataIndex:'business_unit_name'},{title:'消息路由 ID（route_id）',dataIndex:'id'},{title:'路由备注',dataIndex:'remark'},
   {title:'状态',render:(_,r)=><Tag>{r.status==='active'?'启用':'停用'}</Tag>},
   {title:'自动运行',render:(_,r)=>r.retired_at?'已退役':({draft:'配置草稿',running:'运行中',paused:'已暂停'}[r.execution_state??'draft'])},
   {title:'当前凭据授权',render:(_,r)=>links.data?.some(a=>a.route_id===r.id&&a.status==='active')?'已授权':'未授权'},
   {title:'操作',render:(_,r)=>{const active=links.data?.some(a=>a.route_id===r.id&&a.status==='active');return <Space><Button onClick={()=>setConfiguring(r.id)}>执行配置</Button><Button onClick={()=>setReceipts(r.id)}>收件与回执</Button><Button disabled={busy||!!r.retired_at} onClick={()=>open(r)}>编辑路由</Button><Button disabled={busy||!links.data} onClick={()=>toggle(r.id,active?'revoked':'active')}>{active?'撤销授权':'授权'}</Button></Space>}}
  ]}/>
  <Space><Button disabled={page===1} onClick={()=>setPage(page-1)}>上一页</Button><span>{page}</span><Button disabled={!q.data?.has_more} onClick={()=>setPage(page+1)}>下一页</Button></Space>
  <Modal open={editing!==undefined} title={editing?'编辑路由':'新增路由'} onCancel={()=>setEditing(undefined)} onOk={save} confirmLoading={busy} okText="保存" cancelText="取消">
   <Form form={form} layout="vertical">
    <Form.Item name="business_unit_name" label="单位名称" rules={[{validator:(_,value:string|undefined)=>{const name=value?.trim()??'';return name&&Array.from(name).length<=120?Promise.resolve():Promise.reject(new Error('请输入 1 至 120 个字符的单位名称'))}}]}><Input/></Form.Item>
    <Form.Item name="remark" label="路由备注"><Input.TextArea/></Form.Item>
    <Form.Item name="status" label="路由状态"><Select options={[{value:'active',label:'启用'},{value:'disabled',label:'停用'}]}/></Form.Item>
   </Form>
  </Modal>
 <Drawer open={configuring!==null} title="路由执行配置" onClose={()=>setConfiguring(null)} size="large">{configuring&&<Space orientation="vertical" style={{width:'100%'}}><RouteConfigurationEditor key={configuring} route={configuring} onChanged={refresh}/><RouteAdvertising route={configuring}/></Space>}</Drawer>
 <Drawer open={!!receipts} title="收件与回执" onClose={()=>setReceipts(null)} size="large">{receipts&&<SourceReceipts key={receipts} route={receipts}/>}</Drawer>
 </>
}

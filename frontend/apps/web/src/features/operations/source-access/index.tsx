import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Checkbox,Descriptions,Drawer,Form,Input,Popconfirm,Select,Space,Table,Tag,Typography} from 'antd'
import {sourceAccess,type SourceRow,type SourceInput,type Association} from './api'
import {CallerRoutes} from './routes'
import {AssignedRoutes} from './assigned'
import {RoutingDirectory} from './directory'
import {CallerSecret} from './secret'
export function SourceAccess({platformAdmin=true}:{platformAdmin?:boolean}) {return platformAdmin?<CallerCredentials/>:<AssignedRoutes/>}
function CallerCredentials() {
 const kind='source-credentials' as const
 const [directory,setDirectory]=useState(false)
 const [routeCaller,setRouteCaller]=useState<SourceRow|null>(null)
 const [page,setPage]=useState(1),[search,setSearch]=useState(''),[status,setStatus]=useState(''),[source,setSource]=useState(''),[expiry,setExpiry]=useState('')
 const [editing,setEditing]=useState<SourceRow|null>(),[error,setError]=useState(''),[busy,setBusy]=useState(false),[links,setLinks]=useState<Association[]>([])
 const [form]=Form.useForm<SourceInput & {long_term:boolean}>()
 const longTerm=Form.useWatch('long_term',form)
 const q=useQuery({queryKey:['source-admin',kind,page,search,status,source,expiry],queryFn:()=>sourceAccess.list(kind,page,search,status,source,expiry)})
 const open=async(row:SourceRow|null)=>{
 setError('');setLinks([]);form.resetFields();
 const routes:Association[]=[]
 if(row) {try {for(let p=1;;p++){const v=await sourceAccess.routes(row.id,p);routes.push(...v.items);if(!v.has_more)break}}catch(e){setError((e as Error).message);return}}
 setLinks(routes);setEditing(row);form.setFieldsValue({...row,status:row?.status??'active',valid_from:row?.valid_from??new Date().toISOString(),valid_until:row?.valid_until??null,long_term:row?.valid_until==null})
 }
 const save=async()=>{setBusy(true);setError('');try {
 const {long_term,...v}=await form.validateFields()
 const input:SourceInput={...v,valid_until:long_term?null:v.valid_until,...(editing?{revision:editing.revision}:{})}
 await sourceAccess.save(kind,input,editing?.id);setEditing(undefined);await q.refetch()
 }catch(e){setError((e as Error).message??'请核对输入')}finally{setBusy(false)}}
 const act=async(action:()=>Promise<unknown>)=>{setError('');try{await action();await q.refetch()}catch(e){setError((e as Error).message)}}
 return <Card title="调用方凭据" extra={<Space><Button onClick={()=>setDirectory(true)}>业务单位与分组</Button><Button onClick={()=>open(null)}>新增</Button></Space>}>
 {(error||q.error)&&<Alert type="error" title={error||q.error?.message}/>}
 <Alert type="info" title="每个下游对应一条凭据：key_id 是唯一的调用方凭据 ID，secret 是配套签名密钥。下游只需填写 WeKeeper 地址、key_id 和 secret。"/>
 <Space wrap><Input.Search aria-label="搜索调用方" onSearch={s=>{setSearch(s);setPage(1)}}/><Input aria-label="凭据 ID 筛选" placeholder="调用方凭据 ID（key_id）" value={source} onChange={e=>{setSource(e.target.value);setPage(1)}}/><Input aria-label="有效期截止筛选" placeholder="失效时间不晚于（UTC）" onBlur={e=>{setExpiry(e.target.value);setPage(1)}}/><Select aria-label="状态筛选" value={status} onChange={s=>{setStatus(s);setPage(1)}} options={['','active','disabled','revoked'].map(value=>({value,label:value||'全部状态'}))}/></Space>
 <Table<SourceRow> rowKey="id" loading={q.isFetching} dataSource={q.data?.items??[]} pagination={false} columns={[
 {title:'平台名称',render:(_,r)=><Button type="link" onClick={()=>open(r)}>{r.unit_name??r.route_name}</Button>},
 {title:'凭据 ID（key_id）',render:(_,r)=><Typography.Text copyable>{r.id}</Typography.Text>},{title:'备注',dataIndex:'remark'},
 {title:'状态',render:(_,r)=><Tag>{r.status}</Tag>},{title:'有效期至',render:(_,r)=>r.valid_until??'长期有效'},
 {title:'最近调用',dataIndex:'last_used_at'},
 {title:'操作',render:(_,r)=><Space><Button onClick={()=>open(r)}>详情 / 编辑</Button><Button onClick={()=>setRouteCaller(r)}>消息分发路由</Button><Popconfirm title="删除调用方凭据？" description="将永久删除凭据及其消息分发路由，历史收件和任务保留。" onConfirm={()=>act(()=>sourceAccess.remove(r.id))} okText="确认删除" cancelText="取消"><Button danger>删除凭据</Button></Popconfirm></Space>}
 ]}/>
 <Space><Button disabled={page===1} onClick={()=>setPage(page-1)}>上一页</Button><span>{page}</span><Button disabled={!q.data?.has_more} onClick={()=>setPage(page+1)}>下一页</Button></Space>
 <Drawer open={editing!==undefined} title={editing?'详情 / 编辑':'新增'} onClose={()=>setEditing(undefined)} size="large">
 {editing&&<Descriptions title="下游连接配置" column={1} items={[
 {key:'id',label:'凭据 ID（key_id）',children:<Typography.Text copyable>{editing.id}</Typography.Text>},
 {key:'secret',label:'签名密钥（secret）',children:<CallerSecret key={editing.id} keyId={editing.id}/>},
 ]}/>}
 {editing&&<Descriptions title="记录信息" column={1} items={[{key:'revision',label:'版本',children:editing.revision},{key:'created',label:'创建时间',children:editing.created_at},{key:'updated',label:'更新时间',children:editing.updated_at}]}/>}
 <Alert type="info" title="下游填写 WeKeeper 服务地址、key_id 和 secret（点击“查看密钥”取得）；消息路由另选本凭据的 route_id。WeKeeper 不回调或检查下游在线状态。删除后凭据及其路由无法恢复，历史收件与任务保留。"/>
 <Form form={form} layout="vertical">
 <Form.Item name="unit_name" label="平台名称" rules={[{required:true}]}><Input/></Form.Item>
 <Form.Item name="remark" label="凭据备注"><Input.TextArea/></Form.Item>
 <Form.Item name="status" label="状态"><Select options={['active','disabled','revoked'].map(value=>({value,label:value}))}/></Form.Item>
 <><Form.Item name="valid_from" label="生效时间 UTC" rules={[{required:true}]}><Input/></Form.Item><Form.Item name="long_term" valuePropName="checked"><Checkbox>长期有效</Checkbox></Form.Item>{!longTerm&&<Form.Item name="valid_until" label="失效时间 UTC" rules={[{required:true,message:'请输入失效时间 UTC'}]}><Input/></Form.Item>}<Table rowKey="route_id" dataSource={links} pagination={{pageSize:10}} columns={[{title:'消息路由 ID（route_id）',dataIndex:'route_id'},{title:'路由名称',dataIndex:'route_name'},{title:'路由备注',dataIndex:'remark'},{title:'关联状态',dataIndex:'status'}]}/></>
 <Button type="primary" loading={busy} onClick={save}>保存</Button>
 </Form>
 </Drawer>
 <Drawer open={routeCaller!==null} title={`消息分发路由 · ${routeCaller?.unit_name??''}`} onClose={()=>setRouteCaller(null)} size="large">
 {routeCaller&&<CallerRoutes key={routeCaller.id} caller={routeCaller} onChanged={async()=>{await q.refetch()}}/>}
 </Drawer>
 <Drawer open={directory} title="业务单位与分组" onClose={()=>setDirectory(false)} size="large">{directory&&<RoutingDirectory/>}</Drawer>
 </Card>
}

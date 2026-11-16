import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Input,Space,Table,Tabs} from 'antd'
import {sourceRouting} from './api'
import {ReceivingConfigurations} from '../receiving-configurations'
export function RouteConfigurationEditor({route,onChanged,manageAuthorizations=true}:{route:string;onChanged:()=>Promise<void>;manageAuthorizations?:boolean}){
 const q=useQuery({queryKey:['route-configuration',route],queryFn:()=>sourceRouting.configuration(route)})
 const [username,setUsername]=useState(''),[error,setError]=useState('')
 const grant=async(name:string,granted:boolean)=>{try{await sourceRouting.manager(route,name,granted);await q.refetch();await onChanged()}catch(e){setError((e as Error).message)}}
 return <Space orientation="vertical" style={{width:'100%'}}>
 {(q.error||error)&&<Alert type="error" title={q.error?.message||error}/>}
 {q.data&&<><p>业务单位：{q.data.name}</p><Tabs items={(['mobile','wecom'] as const).map(channel=>({key:channel,label:channel==='mobile'?'手机分发':'企业微信',children:<ReceivingConfigurations channel={channel} scope={q.data!.business_unit_id} admin={manageAuthorizations} platformAdmin={manageAuthorizations}/>}))}/></>}
 {manageAuthorizations&&<><h3>消息查看授权</h3><Space><Input aria-label="路由配置管理员用户名" value={username} onChange={e=>setUsername(e.target.value)}/><Button disabled={!username.trim()} onClick={()=>void grant(username,true)}>授权查看消息</Button></Space><Table rowKey="id" dataSource={q.data?.managers??[]} pagination={false} columns={[{title:'用户名',dataIndex:'username'},{title:'操作',render:(_,r)=><Button onClick={()=>void grant(r.username,false)}>撤销授权</Button>}]}/></>}
 </Space>
}

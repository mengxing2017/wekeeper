import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Drawer,Table} from 'antd'
import {sourceRouting} from './api'
import {RouteConfigurationEditor} from './configuration'
export function AssignedRoutes({platformAdmin=false}:{platformAdmin?:boolean}){
 const [route,setRoute]=useState<string|null>(null)
 const q=useQuery({queryKey:['assigned-source-routes'],queryFn:()=>sourceRouting.assigned()})
 return <Card title="路由执行配置">
  {q.error&&<Alert type="error" title={q.error.message}/>}
  <Table rowKey="id" loading={q.isFetching} dataSource={q.data?.items??[]} pagination={{pageSize:20}} columns={[
   {title:'调用方',dataIndex:'caller_name'},{title:'路由名称',dataIndex:'route_name'},{title:'消息路由 ID',dataIndex:'id'},
   {title:'状态',render:(_,r)=>r.retired_at?'已退役':({draft:'配置草稿',running:'运行中',paused:'已暂停'}[r.execution_state])},
   {title:'操作',render:(_,r)=><Button onClick={()=>setRoute(r.id)}>执行配置</Button>}
  ]}/>
  <Drawer open={!!route} title="路由执行配置" onClose={()=>setRoute(null)} size="large">{route&&<RouteConfigurationEditor key={route} route={route} manageAuthorizations={platformAdmin} onChanged={async()=>{await q.refetch()}}/>}</Drawer>
 </Card>
}

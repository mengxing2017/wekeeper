import {RouteAdvertising} from '../advertising'
import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Drawer,Space,Table} from 'antd'
import {sourceRouting} from './api'
import {SourceReceipts} from './receipts'
import {RouteConfigurationEditor} from './configuration'
export function AssignedRoutes({platformAdmin=false,receiptsOnly=false,advertisingOnly=false}:{platformAdmin?:boolean;receiptsOnly?:boolean;advertisingOnly?:boolean}){
 const [receipts,setReceipts]=useState<string|null>(receiptsOnly?new URLSearchParams(window.location.search).get('route'):null)
 const [route,setRoute]=useState<string|null>(null)
 const q=useQuery({queryKey:['assigned-source-routes'],queryFn:()=>sourceRouting.assigned()})
 return <Card title={advertisingOnly?'广告序列轮询':receiptsOnly?'收件与回执':'业务单位接收配置'}>
  {advertisingOnly&&<Alert type="info" title="广告按业务单位、平台、全局投放范围匹配；轮询进度按业务单位计数。"/>}
  {receiptsOnly&&<Alert type="info" title="选择路由查看收件、处理和发送结果。仅显示当前账号有权访问的路由。"/>}
  {q.error&&<Alert type="error" title={q.error.message}/>}
  <Table rowKey="id" loading={q.isFetching} dataSource={q.data?.items??[]} pagination={{pageSize:20}} columns={[
   {title:'调用方',dataIndex:'caller_name'},{title:'路由名称',dataIndex:'route_name'},{title:'消息路由 ID',dataIndex:'id'},
   {title:'状态',render:(_,r)=>r.status==='active'?'启用':'停用'},
   {title:'操作',render:(_,r)=><Space>{!receiptsOnly&&<Button onClick={()=>setRoute(r.id)}>{advertisingOnly?'管理广告投放':'执行配置'}</Button>}<Button href={`/admin?view=source-automation-tasks&route=${encodeURIComponent(r.id)}`}>自动链路任务</Button><Button onClick={()=>setReceipts(r.id)}>{receiptsOnly?'查看收件与回执':'收件与回执'}</Button></Space>}
  ]}/>
  <Drawer open={!!route} title={advertisingOnly?'广告序列轮询配置':'业务单位接收配置'} onClose={()=>setRoute(null)} size="large">{route&&<Space orientation="vertical" style={{width:'100%'}}>{!advertisingOnly&&<RouteConfigurationEditor key={route} route={route} manageAuthorizations={platformAdmin} onChanged={async()=>{await q.refetch()}}/>}<RouteAdvertising key={route} route={route}/></Space>}</Drawer>
  <Drawer open={!!receipts} title="收件与回执" onClose={()=>setReceipts(null)} size="large">{receipts&&<SourceReceipts key={receipts} route={receipts}/>}</Drawer>
 </Card>
}

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
 return <Card title={advertisingOnly?'广告序列轮询':receiptsOnly?'收件与回执':'路由执行配置'}>
  {advertisingOnly&&<Alert type="info" title="选择路由配置广告自动序列轮询：添加或调整独立槽位，发布序列，再将广告模式保存为“序列广告池”。每条路由独立轮询。"/>}
  {receiptsOnly&&<Alert type="info" title="选择路由查看收件、版本处理和发送结果。仅显示当前账号有权访问的路由。"/>}
  {q.error&&<Alert type="error" title={q.error.message}/>}
  <Table rowKey="id" loading={q.isFetching} dataSource={q.data?.items??[]} pagination={{pageSize:20}} columns={[
   {title:'调用方',dataIndex:'caller_name'},{title:'路由名称',dataIndex:'route_name'},{title:'消息路由 ID',dataIndex:'id'},
   {title:'状态',render:(_,r)=>r.retired_at?'已退役':({draft:'配置草稿',running:'运行中',paused:'已暂停'}[r.execution_state])},
   {title:'操作',render:(_,r)=><Space>{!receiptsOnly&&<Button onClick={()=>setRoute(r.id)}>{advertisingOnly?'配置广告序列轮询':'执行配置'}</Button>}<Button href={`/admin?view=source-automation-tasks&route=${encodeURIComponent(r.id)}`}>自动链路任务</Button><Button onClick={()=>setReceipts(r.id)}>{receiptsOnly?'查看收件与回执':'收件与回执'}</Button></Space>}
  ]}/>
  <Drawer open={!!route} title={advertisingOnly?'广告序列轮询配置':'路由执行配置'} onClose={()=>setRoute(null)} size="large">{route&&<Space orientation="vertical" style={{width:'100%'}}>{!advertisingOnly&&<RouteConfigurationEditor key={route} route={route} manageAuthorizations={platformAdmin} onChanged={async()=>{await q.refetch()}}/>}<RouteAdvertising key={route} route={route}/></Space>}</Drawer>
  <Drawer open={!!receipts} title="收件与回执" onClose={()=>setReceipts(null)} size="large">{receipts&&<SourceReceipts key={receipts} route={receipts}/>}</Drawer>
 </Card>
}

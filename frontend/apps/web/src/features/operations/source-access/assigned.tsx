import { TableText, TableIdentifier } from '../../../shared/table-text'
import { TableRowActions } from '../../../shared/table-row-actions'
import {RouteAdvertising} from '../advertising'
import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Card,Drawer,Space,Table} from 'antd'
import {sourceRouting} from './api'
import {SourceReceipts} from './receipts'
import {RouteConfigurationEditor} from './configuration'
export function AssignedRoutes({receiptsOnly=false}:{receiptsOnly?:boolean}){
 const [receipts,setReceipts]=useState<string|null>(receiptsOnly?new URLSearchParams(window.location.search).get('route'):null)
 const [route,setRoute]=useState<string|null>(null)
 const q=useQuery({queryKey:['assigned-source-routes'],queryFn:()=>sourceRouting.assigned()})
 return <Card title={receiptsOnly?'收件与回执':'业务单位接收配置'}>
  {receiptsOnly&&<Alert type="info" title="选择路由查看收件、处理和发送结果。系统管理员可查看全部路由。"/>}
  {q.error&&<Alert type="error" title={q.error.message}/>}
  <Table rowKey="id" loading={q.isFetching} dataSource={q.data?.items??[]} pagination={{pageSize:20}} tableLayout="fixed" scroll={{x:600}} columns={[
   {title:'调用方',width:160,dataIndex:'caller_name',render:value=><TableText value={value} limit={24}/>},{title:'路由名称 / ID',render:(_,r)=><div className="table-record"><TableText value={r.route_name} limit={24}/><TableIdentifier value={r.id}/></div>},
   {title:'状态',width:80,render:(_,r)=>r.status==='active'?'启用':'停用'},
   {title:'操作',width:160,render:(_,r)=><TableRowActions rowLabel={r.route_name} primaryCount={1} actions={[
    receiptsOnly?{key:'receipts',label:'收件与回执',onClick:()=>setReceipts(r.id)}:{key:'configuration',label:'执行配置',onClick:()=>setRoute(r.id)},
    {key:'automation',label:'自动链路任务',href:`/admin?view=source-automation-tasks&route=${encodeURIComponent(r.id)}`},
    !receiptsOnly&&{key:'receipts',label:'收件与回执',onClick:()=>setReceipts(r.id)},
   ]}/>}
  ]}/>
  <Drawer open={!!route} title="业务单位接收配置" onClose={()=>setRoute(null)} size="large">{route&&<Space orientation="vertical" style={{width:'100%'}}><RouteConfigurationEditor key={route} route={route}/><RouteAdvertising key={route} route={route}/></Space>}</Drawer>
  <Drawer open={!!receipts} title="收件与回执" onClose={()=>setReceipts(null)} size="large">{receipts&&<SourceReceipts key={receipts} route={receipts}/>}</Drawer>
 </Card>
}

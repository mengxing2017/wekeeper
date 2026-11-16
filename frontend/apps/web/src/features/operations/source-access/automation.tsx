import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Space,Table,Typography} from 'antd'
import {businessMessages,receiptReasonLabels} from './api'
export function SourceAutomationTasks(){
 const [page,setPage]=useState(1),route=new URLSearchParams(window.location.search).get('route')??''
 const q=useQuery({queryKey:['business-messages',route,page],queryFn:()=>businessMessages.list(route,'',page),refetchInterval:5000})
 return <Card title="即时消息与发送结果" extra={<Button loading={q.isFetching} onClick={()=>void q.refetch()}>刷新</Button>}><Space orientation="vertical" style={{width:'100%'}}>
 <Alert type="info" title="每条消息独立处理" description="配置和广告变更只影响新消息；没有目标群的消息直接显示未下发成功。"/>
 {q.error&&<Alert type="error" title={q.error.message}/>}
 <Table rowKey="id" dataSource={q.data?.items??[]} pagination={false} expandable={{expandedRowRender:r=><><Typography.Paragraph style={{whiteSpace:'pre-wrap'}}>{r.content}</Typography.Paragraph><Table rowKey="id" dataSource={r.delegates} pagination={false} columns={[{title:'通道',dataIndex:'channel'},{title:'通道任务',dataIndex:'task_id'},{title:'目标数',dataIndex:'target_count'}]}/></>}} columns={[{title:'消息 ID',dataIndex:'event_id'},{title:'状态',render:(_,r)=>({received:'处理中',preparing:'处理中',ready:'处理中',delegated:'通道已受理',failed:'未下发成功',historical:'历史任务'}[r.state]??r.state)},{title:'原因',render:(_,r)=>r.reason_code?(receiptReasonLabels[r.reason_code]??r.reason_code):'—'},{title:'发送结果',render:(_,r)=>r.sending?`成功 ${r.sending.sent} / 失败 ${r.sending.failed} / 待处理 ${r.sending.pending} / 不确定 ${r.sending.unknown}`:'—'},{title:'接收时间',dataIndex:'created_at'}]}/>
 <Space><Button disabled={page===1} onClick={()=>setPage(page-1)}>上一页</Button><Button disabled={!q.data?.has_more} onClick={()=>setPage(page+1)}>下一页</Button></Space>
 </Space></Card>
}

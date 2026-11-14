import {useState} from 'react'
import {PaginationControls} from '../../../shared/pagination-controls'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Table} from 'antd'
import {sourceReceipts,receiptReasonLabels,receiptStageLabels} from './api'
import {AdCompositions} from '../advertising'
export function SourceReceipts({route}:{route:string}){
 const [page,setPage]=useState(1)
 const q=useQuery({queryKey:['source-receipts',route,page],queryFn:()=>sourceReceipts.list(route,page),refetchInterval:5000})
 return <>{q.error&&<Alert type="error" title={q.error.message}/>}<Button onClick={()=>void q.refetch()}>刷新收件</Button><Table rowKey="id" dataSource={q.data?.items??[]} pagination={false} expandable={{expandedRowRender:r=><AdCompositions route={route} receipt={r.id}/>}} columns={[{title:'消息 ID',dataIndex:'event_id'},{title:'处理结果',render:(_,r)=>receiptStageLabels[r.processing_stage]??r.processing_stage},{title:'原因',render:(_,r)=>r.reason_code?(receiptReasonLabels[r.reason_code]??r.reason_code):'—'},{title:'接收时间',dataIndex:'received_at'}]}/><PaginationControls page={page} more={q.data?.has_more} busy={q.isFetching} change={setPage} subject="收件"/></>
}

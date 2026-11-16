import { formatDate } from '../../../shared/format-date'
import { TableText, TableIdentifier } from '../../../shared/table-text'
import {useState} from 'react'
import {PaginationControls} from '../../../shared/pagination-controls'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Table} from 'antd'
import {sourceReceipts,receiptReasonLabels,receiptStageLabels} from './api'
import {AdCompositions} from '../advertising'
export function SourceReceipts({route}:{route:string}){
 const [page,setPage]=useState(1)
 const q=useQuery({queryKey:['source-receipts',route,page],queryFn:()=>sourceReceipts.list(route,page),refetchInterval:5000})
 return <>{q.error&&<Alert type="error" title={q.error.message}/>}<Button onClick={()=>void q.refetch()}>刷新</Button><Table tableLayout="fixed" scroll={{x:700}} rowKey="id" dataSource={q.data?.items??[]} pagination={false} expandable={{expandedRowRender:r=><AdCompositions route={route} receipt={r.id}/>}} columns={[{title:'消息 ID', width:200,dataIndex:'event_id', render:value=><TableIdentifier value={value}/>},{title:'处理结果', width:120,render:(_,r)=>receiptStageLabels[r.processing_stage]??r.processing_stage},{title:'原因',render:(_,r)=><TableText value={r.reason_code?(receiptReasonLabels[r.reason_code]??r.reason_code):'—'}/>},{title:'接收时间', width:180,dataIndex:'received_at',render:value=>formatDate(value)}]}/><PaginationControls page={page} more={q.data?.has_more} busy={q.isFetching} change={setPage} subject="收件"/></>
}

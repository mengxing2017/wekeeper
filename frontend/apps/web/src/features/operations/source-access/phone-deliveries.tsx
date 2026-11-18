import {useState} from 'react'
import {useMutation,useQuery} from '@tanstack/react-query'
import {Alert,Button,Popconfirm,Space,Table,Typography} from 'antd'
import {PaginationControls} from '../../../shared/pagination-controls'
import {businessMessages} from './api'
const outcomes:Record<string,string>={pending:'处理中',sent:'手机反馈发送成功',failed:'未完成发送',unknown:'结果未知'}
export function PhoneDeliveries({receipt}:{receipt:string}){
 const [page,setPage]=useState(1)
 const query=useQuery({queryKey:['phone-responsibilities',receipt,page],queryFn:()=>businessMessages.phoneDeliveries(receipt,page),refetchInterval:5000})
 const stop=useMutation({mutationFn:(target?:string)=>businessMessages.stopPhones(receipt,target),onSuccess:()=>query.refetch()})
 const s=query.data?.summary
 return <Space orientation="vertical" style={{width:'100%'}}>
 <Typography.Title level={5}>手机独立投递结果</Typography.Title>
 {(query.error||stop.error)&&<Alert type="error" title={query.error?.message??stop.error?.message}/>}
 {s&&<><Typography.Text>{s.target_count} 个群，{s.expected_count} 个手机投递项；已生成 {s.materialized_count} / {s.expected_count}，成功 {s.sent}、失败 {s.failed}、未知 {s.unknown}、待处理 {s.pending}</Typography.Text>
 {s.sent>0&&s.sent<s.expected_count&&<Alert type="info" title="部分成功，其他手机的责任仍独立保留"/>}
 {s.target_count>0&&<Popconfirm title="停止此消息后续手机下发？已交付的执行包仍可能执行。" onConfirm={()=>stop.mutateAsync(undefined)}><Button danger loading={stop.isPending}>停止此消息手机下发</Button></Popconfirm>}</>}
 <Table rowKey="id" dataSource={query.data?.targets??[]} pagination={false} columns={[
 {title:'目标群',dataIndex:'target_name'},
 {title:'本群结果',render:(_,r)=>outcomes[r.outcome]??r.outcome},
 {title:'手机投递项',render:(_,r)=>`${r.materialized_count}/${r.expected_count} 已生成；成功 ${r.sent} / 失败 ${r.failed} / 未知 ${r.unknown} / 待处理 ${r.pending}`},
 {title:'操作',render:(_,r)=>r.stopped?'已停止自动下发':<Popconfirm title="停止本群后续手机下发？" onConfirm={()=>stop.mutateAsync(r.target_id)}><Button danger>停止本群</Button></Popconfirm>},
 ]}/>
 <Table rowKey="id" dataSource={query.data?.items??[]} pagination={false} columns={[
 {title:'目标群',dataIndex:'target_name'},
 {title:'指定手机',render:(_,r)=>r.device_name??'候选内轮询及自动换机'},
 {title:'执行结果',render:(_,r)=>outcomes[r.outcome]??r.outcome},
 {title:'当前阶段与原因',render:(_,r)=><>{r.stage} / {r.reason_code}{r.historical_risk&&'；历史结果未知'}</>},
 {title:'执行记录',render:(_,r)=>r.task_id?<Button type="link" href={`/admin?view=mobile-execution-tasks&task=${encodeURIComponent(r.task_id)}`}>查看任务与逐次反馈</Button>:'尚未创建任务'},
 ]}/>
 <PaginationControls page={page} more={query.data?.has_more} busy={query.isFetching} change={setPage} subject="手机独立投递项"/>
 </Space>
}

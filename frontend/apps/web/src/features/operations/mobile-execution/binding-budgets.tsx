import {useState} from 'react'
import {useMutation,useQuery,useQueryClient} from '@tanstack/react-query'
import {Alert,Button,Input,InputNumber,Modal,Space,Switch,Table} from 'antd'
import {api,type TargetBinding} from './api'
import {createIdempotencyKey} from '../../../shared/idempotency-key'
import {PaginationControls} from '../../../shared/pagination-controls'
export function BindingBudgets(){
 const cache=useQueryClient(),[page,setPage]=useState(1),[row,setRow]=useState<TargetBinding>(),[limit,setLimit]=useState<number|null>(null),[until,setUntil]=useState(''),[enabled,setEnabled]=useState(true)
 const query=useQuery({queryKey:['mobile-execution','binding-budgets',page],queryFn:()=>api.platformBindings(page)})
 const save=useMutation({mutationFn:()=>api.updateBinding(row!.id,{expected_revision:row!.revision,enabled,max_calls:limit,valid_until:until?new Date(until).toISOString():null},createIdempotencyKey()),onSuccess:()=>{setRow(undefined);void cache.invalidateQueries({queryKey:['mobile-execution']})}})
 function edit(r:TargetBinding){setRow(r);setLimit(r.max_calls??null);setEnabled(r.enabled);setUntil(r.valid_until?new Date(Date.parse(r.valid_until)-new Date(r.valid_until).getTimezoneOffset()*60000).toISOString().slice(0,16):'');save.reset()}
 return <><h3>手机与目标群候选绑定</h3><p>同一绑定跨单位共享累计预算。可选额度和期限留空时不附加限制；修改或重新启用不会重置累计消耗。接收配置维护实际消息范围。</p>
 {query.error&&<Alert type="error" title={query.error.message}/>}
 <Table<TargetBinding> rowKey="id" dataSource={query.data?.items??[]} loading={query.isFetching} pagination={false} columns={[{title:'手机',dataIndex:'device_name'},{title:'目标群',dataIndex:'search_text'},{title:'状态',render:(_,r)=>r.enabled?'启用':'停用'},{title:'累计调用',render:(_,r)=>`${r.used_calls??0} / ${r.max_calls??'未设上限'}`},{title:'有效至',render:(_,r)=>r.valid_until?new Date(r.valid_until).toLocaleString():'未设期限'},{title:'操作',render:(_,r)=><Button onClick={()=>edit(r)}>编辑绑定约束</Button>}]}/>
 <PaginationControls page={page} more={query.data?.page?.has_more} change={setPage} subject="候选绑定"/>
 <Modal title="编辑绑定约束" open={!!row} onCancel={()=>setRow(undefined)} onOk={()=>save.mutate()} confirmLoading={save.isPending}>
 <Space orientation="vertical"><p>{row?.device_name} / {row?.search_text}</p><label>启用候选绑定 <Switch checked={enabled} onChange={setEnabled}/></label><label>累计调用上限 <InputNumber aria-label="绑定累计调用上限" min={1} precision={0} value={limit} onChange={setLimit}/></label><label>有效至 <Input aria-label="绑定有效至" type="datetime-local" value={until} onChange={e=>setUntil(e.target.value)}/></label><Button onClick={()=>setUntil('')}>清除期限</Button>{save.error&&<Alert type="error" title={save.error.message}/>}</Space>
 </Modal></>
}

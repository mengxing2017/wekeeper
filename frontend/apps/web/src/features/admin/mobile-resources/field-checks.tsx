import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Modal, Select, Space, Table } from 'antd'
import { api, type CapabilityOpening, type FieldCheck, type FieldCheckRequest } from './api'
import { PaginationControls } from '../../../shared/pagination-controls'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

export function PlatformFieldChecks({row}:{row:CapabilityOpening}) {
 const client=useQueryClient()
 const [page,setPage]=useState(1)
 const [checkPage,setCheckPage]=useState(1)
 const [invite,setInvite]=useState<string>()
 const [pending,setPending]=useState<{input:FieldCheckRequest;key:string}>()
 const invites=useQuery({queryKey:['mobile-execution','platform','field-invitations',row.id,page],queryFn:()=>api.platformFieldInvitations(page,row.id),refetchInterval:15000})
 const checks=useQuery({queryKey:['mobile-execution','platform','field-checks',row.id,checkPage],queryFn:()=>api.fieldChecks(checkPage,row.id),refetchInterval:15000})
 const save=useMutation({mutationFn:(r:NonNullable<typeof pending>)=>api.requestFieldCheck(r.input,r.key),onSuccess:()=>{setPending(undefined);setInvite(undefined);void client.invalidateQueries({queryKey:['mobile-execution']})}})
 const chosen=invites.data?.items.find(i=>i.id===invite && i.available)
 function request() {
  if(!chosen || !row.opening)return
  const r={input:{opening_id:row.opening.id,invitation_id:chosen.id,expected_invitation_revision:chosen.revision},key:createIdempotencyKey()}
  setPending(r);save.mutate(r)
 }
 return <Card size="small" title="现场无发送检查">
  <p>请先由学校管理员在“手机目标群”邀请一个已绑定目标，再选择该邀请发起检查。手机现场人员仍须刷新检查并在原生弹窗确认。</p>
  {!row.consent_current && <Alert type="warning" title="先完成本机能力同意，再发起检查" />}
  {(invites.error || checks.error || save.error) && <Alert type="error" title="检查请求或状态尚未确认" description={(invites.error || checks.error || save.error)?.message} />}
  <Select aria-label="选择学校检查邀请" style={{width:'100%'}} value={invite} disabled={!!pending} onChange={setInvite} placeholder="选择学校发出的单目标邀请" options={(invites.data?.items ?? []).map(i=>({value:i.id,label:`${i.school_name || '学校'} · ${i.search_text}${i.available ? '' : '（已使用、过期或依据变化）'}`,disabled:!i.available}))} />
  <PaginationControls page={page} more={invites.data?.page?.has_more} change={setPage} subject="获准检查邀请" />
  <Button disabled={!!pending || !chosen || !row.consent_current || !row.opening} onClick={()=>Modal.confirm({title:'发起这次无发送检查？',content:`目标搜索文字：${chosen?.search_text}。本次只提交检查请求，不自动打开手机应用，也不开放任务发送。`,onOk:request})}>发起无发送检查</Button>
  <Table<FieldCheck> rowKey="id" dataSource={checks.data?.items ?? []} loading={checks.isFetching} pagination={false} columns={[
   {title:'检查记录',dataIndex:'id'},
   {title:'进度',render:(_,r)=>r.dependencies_current===false ? '依据已变化，仅保留历史记录' : r.state==='pending' ? new Date(r.expires_at).getTime()<=Date.now() ? '检查过期，学校需重新邀请' : '请在手机刷新并确认开始' : r.state==='leased' ? '手机已领取，等待固定结果' : r.state==='passed' ? '无发送检查通过；任务发送仍未开放' : r.state==='failed' ? '检查未通过，请核对现场页面后重新邀请' : '已过期或失效，请学校重新邀请'},
  ]} />
  <PaginationControls page={checkPage} more={checks.data?.page?.has_more} change={setCheckPage} subject="现场检查记录" />
  {pending && <Space><Button loading={save.isPending} onClick={()=>save.mutate(pending)}>重试同一检查申请</Button><Button disabled={save.isPending} onClick={()=>{setPending(undefined);void checks.refetch();void invites.refetch()}}>重新查询检查记录</Button></Space>}
 </Card>
}

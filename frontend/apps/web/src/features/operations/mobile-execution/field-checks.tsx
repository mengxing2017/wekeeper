import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Modal, Space, Table, Tag } from 'antd'
import { api, type TargetBinding, type FieldCheckInvitation, type FieldCheck } from './api'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { PaginationControls } from '../../../shared/pagination-controls'

type Request = {kind:'invite';row:TargetBinding;key:string}|{kind:'revoke';row:FieldCheckInvitation;key:string}
const states:Record<FieldCheck['state'],string>={pending:'等待手机确认',leased:'手机已领取',passed:'检查通过',failed:'检查未通过',stale:'依据失效，仅保留记录',expired:'检查已过期'}
export function SchoolFieldChecks({school,admin,bindings}:{school:string;admin:boolean;bindings:TargetBinding[]}) {
 const client=useQueryClient()
 const [page,setPage]=useState(1)
 const [checkPage,setCheckPage]=useState(1)
 const [pending,setPending]=useState<Request>()
 const invites=useQuery({queryKey:['mobile-execution',school,'field-invitations',page],queryFn:()=>api.fieldInvitations(school,page),refetchInterval:15000})
 const checks=useQuery({queryKey:['mobile-execution',school,'field-checks',checkPage],queryFn:()=>api.schoolFieldChecks(school,checkPage),refetchInterval:15000})
 const save=useMutation({mutationFn:(r:Request)=>r.kind==='invite' ? api.inviteFieldCheck(school,{binding_id:r.row.id,expected_binding_revision:r.row.revision,expected_target_locator_revision:r.row.target_locator_revision!},r.key) : api.revokeFieldInvitation(school,r.row.id,r.row.revision,r.key),onSuccess:()=>{setPending(undefined);void client.invalidateQueries({queryKey:['mobile-execution']})}})
 function submit(r:Request) {setPending(r);save.mutate(r)}
 return <Card title="本校目标的无发送检查邀请">
  <Alert type="info" showIcon title="邀请只用于一次现场检查" description="学校管理员选择本页已绑定目标，平台管理员引用邀请发起检查，手机现场人员再确认开始。只检查搜索首群和聊天控件，不发送消息；不授权整校设备使用或任务发送。" />
  {(invites.error || checks.error || save.error) && <Alert type="error" title="检查信息或请求尚未确认" description={(invites.error || checks.error || save.error)?.message} />}
  {admin && <Space wrap>{bindings.filter(b=>b.available && b.target_locator_revision!=null).map(b=><Button key={b.id} disabled={!!pending} onClick={()=>Modal.confirm({title:'邀请检查这个目标？',content:`手机：${b.device_name}；有效搜索文字：${b.search_text}。邀请只允许一次无发送检查，有效期 24 小时，不验证真实群身份。`,onOk:()=>submit({kind:'invite',row:b,key:createIdempotencyKey()})})}>邀请检查：{b.target_alias || b.search_text} · {b.device_name}</Button>)}</Space>}
  {!admin && <p>当前可查看本校记录；请由本校管理员发出或撤销邀请。</p>}
  <Table<FieldCheckInvitation> rowKey="id" dataSource={invites.data?.items ?? []} loading={invites.isFetching} pagination={false} columns={[
   {title:'检查目标搜索文字',dataIndex:'search_text'},
   {title:'有效期至',render:(_,r)=>new Date(r.expires_at).toLocaleString()},
   {title:'状态',render:(_,r)=><Tag>{r.revoked_at ? '已撤销' : r.consumed ? '已用于检查' : r.available ? '等待平台发起' : '已过期或依据变化'}</Tag>},
   ...(admin ? [{title:'操作',render:(_:unknown,r:FieldCheckInvitation)=><Button danger disabled={!!pending || !!r.revoked_at} onClick={()=>Modal.confirm({title:'撤销这次检查邀请？',content:'停止后续领取；已运行的检查不强制解锁，原证据保留。',onOk:()=>submit({kind:'revoke',row:r,key:createIdempotencyKey()})})}>撤销邀请</Button>}] : []),
  ]} />
  <PaginationControls page={page} more={invites.data?.page?.has_more} change={setPage} subject="检查邀请" />
  <Table<FieldCheck> rowKey="id" dataSource={checks.data?.items ?? []} loading={checks.isFetching} pagination={false} columns={[
   {title:'检查记录',dataIndex:'id'},
   {title:'状态',render:(_,r)=>r.dependencies_current===false ? '依据已变化，原结果仅作历史记录' : r.state==='pending' && new Date(r.expires_at).getTime()<=Date.now() ? '检查已过期' : states[r.state]},
   {title:'下一步',render:(_,r)=>r.state==='pending' ? '手机刷新检查并原生确认' : r.state==='leased' ? '等待手机固定结果；不要重复执行' : r.state==='passed' && r.dependencies_current ? '仅检查通过，任务发送仍未开放' : '核对手机环境后重新邀请'},
  ]} />
  <PaginationControls page={checkPage} more={checks.data?.page?.has_more} change={setCheckPage} subject="现场检查" />
  {pending && <Space><Button loading={save.isPending} onClick={()=>save.mutate(pending)}>重试同一请求</Button><Button disabled={save.isPending} onClick={()=>{setPending(undefined);void invites.refetch();void checks.refetch()}}>重新查询记录</Button></Space>}
 </Card>
}

import {useState} from 'react'
import {Alert,Button,Card,Drawer,Input,Space,Tag} from 'antd'
import {useMutation,useQuery,useQueryClient} from '@tanstack/react-query'
import {product} from './api'
export function OfficialChannelControl(){
 const client=useQueryClient()
 const status=useQuery({queryKey:['wecom','platform','channel-control'],queryFn:product.officialChannelStatus,refetchInterval:15000})
 const [open,setOpen]=useState(false),[reason,setReason]=useState('')
 const change=useMutation({mutationFn:()=>product.officialChannelControl({paused:!status.data!.paused,expected_revision:status.data!.revision,reason:reason.trim()}),onSuccess:async()=>{setOpen(false);setReason('');await client.invalidateQueries({queryKey:['wecom']})}})
 return <Card title="官方渠道运行状态">
  <p><Tag>{!status.data?(status.isPending?'正在读取运行状态':'状态暂不可读'):status.data.paused?'已暂停新发送':!status.data.configured?'尚未配置企业微信服务端凭据':status.data.health_state==='available'?'官方接口可用':'系统正在检查官方接口'}</Tag></p>
  <p>系统自动检查连接。接口恢复会自动继续已确认且符合条件的工作；人工暂停需要单独恢复。</p>
  {status.data?.health?.checked_at&&<p>最近检查：{new Date(status.data.health.checked_at).toLocaleString()}</p>}
  {status.error&&<Alert type="error" title={status.error.message}/>}
  <Space><Button onClick={()=>void status.refetch()}>刷新状态</Button><Button disabled={!status.data} onClick={()=>{setOpen(true);change.reset()}}>{status.data?.paused?'恢复新发送':'暂停新发送'}</Button></Space>
  <Drawer title={status.data?.paused?'恢复官方渠道新发送':'暂停官方渠道新发送'} open={open} onClose={()=>setOpen(false)} destroyOnHidden>
   <p>此操作只控制官方企业微信渠道，不改变共享手机、历史调用及未知结果。</p>
   <Input.TextArea aria-label="操作原因" value={reason} maxLength={1000} onChange={e=>setReason(e.target.value)} placeholder="填写本次暂停或恢复的业务原因"/>
   {change.error&&<Alert type="error" title={change.error.message}/>}
   <Button type="primary" disabled={!reason.trim()} loading={change.isPending} onClick={()=>change.mutate()}>确认{status.data?.paused?'恢复':'暂停'}</Button>
  </Drawer>
 </Card>
}

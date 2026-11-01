import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Table, Tag, Modal, Form, Select, Input } from 'antd'
import { api, sharedSend, type TargetBinding } from './api'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { PaginationControls } from '../../../shared/pagination-controls'

export function MobileTargetBindings({ school, admin=false }: { school: string; admin?: boolean }) {
  const cache=useQueryClient()
  const shared=useQuery({queryKey:['mobile-execution',school,'shared-inspections'],queryFn:()=>sharedSend.overview(school),refetchInterval:20000})
  const inspect=useMutation({mutationFn:(r:TargetBinding)=>sharedSend.inspection(school,r.target_id,r.device_id,createIdempotencyKey()),onSuccess:()=>cache.invalidateQueries({queryKey:['mobile-execution',school]})})
  const [page, setPage] = useState(1)
  const [open,setOpen]=useState(false)
  const [targetPage,setTargetPage]=useState(1)
  const [targetSearch,setTargetSearch]=useState('')
  const [form]=Form.useForm<{target_id:string;device_id:string;search_text_override?:string}>()
  const targets=useQuery({queryKey:['mobile-execution',school,'pool-targets',targetPage,targetSearch],queryFn:()=>api.targets(school,targetPage,targetSearch),enabled:open})
  const bind=useMutation({mutationFn:async(v:{target_id:string;device_id:string;search_text_override?:string})=>{
    const access=shared.data?.pool.find(d=>d.id===v.device_id)
    if(!access)throw new Error('手机资源资格已变化，请刷新后重选')
    return sharedSend.bind(school,{target_id:v.target_id,device_id:v.device_id,expected_access_revision:access.access_revision,search_text_override:v.search_text_override?.trim()?v.search_text_override:null},createIdempotencyKey())
  },onSuccess:()=>{setOpen(false);form.resetFields();void cache.invalidateQueries({queryKey:['mobile-execution',school]})}})
  const unbind=useMutation({mutationFn:(row:TargetBinding)=>sharedSend.unbind(school,row.id,row.revision,createIdempotencyKey()),onSuccess:()=>cache.invalidateQueries({queryKey:['mobile-execution',school]})})
  const rows = useQuery({ queryKey: ['mobile-execution', school, 'bindings', page], queryFn: () => api.bindings(school, page) })
  return <><Card title="共享手机与目标群绑定" extra={admin && <Button onClick={()=>{form.resetFields();setOpen(true)}}>绑定可用手机</Button>}>
    <Alert type="info" showIcon title="群属于本校，手机属于共享资源池" description="绑定保存后系统自动安排无发送检查。手机只需首次启用托管，无需逐群确认；离线或锁屏时等待恢复。绑定不证明手机已加入真实群，发送仍需学校确认任务及有效验收范围。" />
    {(inspect.error || bind.error || unbind.error) && <Alert type="error" title={(inspect.error || bind.error || unbind.error)?.message} />}
    {inspect.isSuccess && <Alert type="success" title="已重新评估检查需求，符合条件后自动执行，无需手机逐群确认" />}
    {rows.error && <Alert type="error" title={rows.error.message} />}
    <Table<TargetBinding> rowKey="id" dataSource={rows.data?.items ?? []} loading={rows.isFetching} pagination={false} columns={[
      { title: '手机', render: (_, row) => row.device_name || row.device_id },
      { title: '有效搜索文字', render: (_, row) => <span style={{ whiteSpace: 'pre-wrap' }}>{row.search_text}</span> },
      { title: '文字来源', render: (_, row) => row.search_source === 'binding_override' ? '该手机覆盖名' : '目标群默认名' },
      { title: '群别名', dataIndex: 'target_alias' },
      { title: '自动检查与发送条件', render: (_, row) => {
        const state=shared.data?.readiness?.find(r=>r.binding_id===row.id)
        return !state ? '正在核对设备条件' : state.ready ? '可以用于发送任务' : <ul>{state.blockers.map(b=><li key={b.code}>{b.message}（{({platform:'平台管理员',school:'学校管理员',device:'手机现场人员',system:'系统自动处理',engineering:'产品研发',administrator:'管理员'} as Record<string,string>)[b.owner] ?? b.owner}）</li>)}</ul>
      } },
      { title: '绑定管理', render:(_,row)=>admin && row.source==='school_pool' && row.enabled ? <Button danger onClick={()=>Modal.confirm({title:'停用本校目标绑定？',onOk:()=>unbind.mutateAsync(row)})}>停用绑定</Button> : '平台维护' },
      { title: '诊断', render: (_, row) => <Button disabled={!admin || !row.enabled || inspect.isPending} onClick={()=>inspect.mutate(row)}>重新评估自动检查</Button> },
      { title: '状态', render: (_, row) => <Tag>{!row.enabled ? '绑定已停用' : row.available ? '资源有效，任务仍需确认及调度' : '设备或目标暂不可用'}</Tag> },
    ]} />
    <PaginationControls page={page} more={rows.data?.page?.has_more} change={setPage} subject="设备目标绑定" />
  <Modal title="绑定本校可用手机" open={open} confirmLoading={bind.isPending} onCancel={()=>setOpen(false)} onOk={()=>void form.validateFields().then(v=>bind.mutate(v))}>
    <p>只能选择平台已批准本校使用的手机；保存后自动检查，无需手机再次同意。</p>
    <Form form={form} layout="vertical">
      <Form.Item name="target_id" label="本校目标群" rules={[{required:true}]}><Select showSearch filterOption={false} onSearch={v=>{setTargetSearch(v);setTargetPage(1)}} options={targets.data?.items.filter(t=>t.enabled).map(t=>({value:t.id,label:t.alias||t.search_text}))}/></Form.Item>
      <PaginationControls page={targetPage} more={targets.data?.page?.has_more} change={setTargetPage} subject="可绑定目标群"/>
      <Form.Item name="device_id" label="已批准共享手机" rules={[{required:true}]}><Select options={shared.data?.pool?.map(d=>({value:d.id,label:d.name}))}/></Form.Item>
      <Form.Item name="search_text_override" label="此手机内搜索名（可选，留空使用群默认名）"><Input maxLength={256}/></Form.Item>
    </Form>
    {!shared.data?.pool?.length && <Alert type="info" title="暂无可用手机，请平台管理员一次配置本校设备池资格"/>}
  </Modal>
  </Card></>
}

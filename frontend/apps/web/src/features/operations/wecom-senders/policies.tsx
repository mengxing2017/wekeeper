import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Card, InputNumber, Select, Space, Switch, Table } from 'antd'
import { distributionPolicies, product, type SourcePolicy, type MemberPolicyInput, type BusinessPreview } from './api'

export function DistributionPolicyPanel() {
  const query = useQuery({ queryKey: ['wecom', 'platform', 'policies'], queryFn: distributionPolicies.get })
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [source, setSource] = useState<SourcePolicy>()
  const [resource, setResource] = useState<MemberPolicyInput>()
  const members=useQuery({queryKey:['official-policy-members'],queryFn:()=>product.officialUsage.options()})
  const [preview,setPreview]=useState<BusinessPreview<MemberPolicyInput>>()
  const save = async () => {
    setBusy(true); setError('')
    try {
      if (source) await distributionPolicies.source(source)
      else if (resource) {setPreview(await product.memberPolicies.preview(resource));return}
      setSource(undefined); setResource(undefined); await query.refetch()
    } catch (e) { setError(e instanceof Error ? e.message : '政策更新失败') }
    finally { setBusy(false) }
  }
  return <Card title="分发资源治理（平台权限）">
    <p>来源积压按来源累计，官方资源调用量跨来源和学校累计。这里的额度是平台运营限制，不代表官方安全发送次数。手机业务状态在手机使用管理中查看。</p>
    {(error || query.error) && <Alert type="error" title={error || query.error?.message} />}
    <Table<SourcePolicy> rowKey="namespace" pagination={false} dataSource={query.data?.sources ?? []} columns={[
      { title: '受信来源', render: (_,r) => ({'builtin.forum':'校园社区','builtin.direct':'直接通知'}[r.namespace]??'已登记业务来源') }, { title: '草稿/待执行限制', dataIndex: 'max_pending' },
      { title: '状态', render: (_, row) => row.enabled ? '允许受理' : '已暂停新发送' },
      { title: '操作', render: (_, row) => <Button onClick={() => { setSource({ ...row }); setResource(undefined) }}>调整来源政策</Button> },
    ]} />
    <Table rowKey="id" dataSource={members.data?.members??[]} pagination={{pageSize:10}} columns={[
      {title:'官方成员',dataIndex:'name'},{title:'窗口',render:(_,r)=>r.policy?`${r.policy.window_seconds} 秒`:'沿用渠道限制'},
      {title:'次数',render:(_,r)=>r.policy?`${r.used??0} / ${r.policy.max_calls}`:'沿用渠道限制'},
      {title:'额度恢复时间',dataIndex:'restores_at'},
      {title:'操作',render:(_,r)=><Button onClick={()=>{setSource(undefined);setPreview(undefined);setResource({member_id:r.id,generation:members.data!.generation!,revision:r.policy?.revision??0,enabled:r.policy?.enabled??true,window_seconds:r.policy?.window_seconds??3600,max_calls:r.policy?.max_calls??1})}}>调整成员限制</Button>}
    ]}/>
    {(source || resource) && <Space orientation="vertical">
      {source && <><p>{source.namespace}</p><label>来源积压限制<InputNumber aria-label="来源积压限制" min={1} max={100000} value={source.max_pending} onChange={v => setSource({ ...source, max_pending: Number(v) })} /></label><Switch aria-label="来源允许新发送" checked={source.enabled} onChange={enabled => setSource({ ...source, enabled })} /></>}
      {resource && !preview && <><label>官方成员<Select aria-label="受限官方成员" value={resource.member_id} options={members.data?.members.map(v=>({value:v.id,label:v.name}))} onChange={id=>{const m=members.data?.members.find(v=>v.id===id);setResource({...resource,member_id:id,revision:m?.policy?.revision??0})}}/></label><label>窗口秒数<InputNumber aria-label="窗口秒数" min={1} max={2592000} value={resource.window_seconds} onChange={v=>setResource({...resource,window_seconds:Number(v)})}/></label><label>窗口调用数<InputNumber aria-label="窗口调用数" min={1} max={100000} value={resource.max_calls} onChange={v=>setResource({...resource,max_calls:Number(v)})}/></label><Switch aria-label="资源允许新发送" checked={resource.enabled} onChange={enabled=>setResource({...resource,enabled})}/></>}
      {preview&&<><p>成员：{members.data?.members.find(v=>v.id===preview.summary.member_id)?.name} · {preview.summary.window_seconds} 秒内最多 {preview.summary.max_calls} 次 · {preview.summary.enabled?'启用':'暂停'}。同一成员跨学校和内容来源累计，变更不会清空已用额度。</p><Button disabled={busy} onClick={()=>setPreview(undefined)}>返回修改</Button><Button type="primary" loading={busy} onClick={()=>{setBusy(true);setError('');void product.memberPolicies.confirm({preview_id:preview.id,digest:preview.digest}).then(async()=>{setPreview(undefined);setResource(undefined);await members.refetch()}).catch(e=>setError(e instanceof Error?e.message:'更新失败')).finally(()=>setBusy(false))}}>确认限额生效</Button></>}

      <Space><Button type="primary" loading={busy} disabled={!!preview} onClick={() => void save()}>{resource?'核对限制':'保存政策'}</Button><Button onClick={() => { setSource(undefined); setResource(undefined) }}>取消调整</Button></Space>
    </Space>}
  </Card>
}

import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Form, Input, InputNumber, Modal, Select, Space, Table } from 'antd'
import { sharedSend, type SharedPolicyInput } from './api'
import { createIdempotencyKey } from '../../../shared/idempotency-key'

export function SharedSendReadiness() {
  const cache = useQueryClient()
  const status = useQuery({ queryKey: ['mobile-execution', 'platform', 'shared-readiness'], queryFn: sharedSend.platform, refetchInterval: 20_000 })
  const [accessOpen,setAccessOpen]=useState(false)
  const [accessForm]=Form.useForm<{school_id:string;device_id:string}>()
  const access=useMutation({mutationFn:(v:{school_id:string;device_id:string;enabled?:boolean})=>sharedSend.access({...v,enabled:v.enabled??true,expected_revision:status.data?.access?.find(a=>a.school_id===v.school_id&&a.device_id===v.device_id)?.revision??0},createIdempotencyKey()),onSuccess:()=>{setAccessOpen(false);void refresh()}})
  const [editing, setEditing] = useState(false)
  const [form] = Form.useForm<SharedPolicyInput>()
  const refresh = () => cache.invalidateQueries({ queryKey: ['mobile-execution'] })
  const policy = useMutation({ mutationFn: (v: SharedPolicyInput) => sharedSend.policy(v, createIdempotencyKey()), onSuccess: () => { setEditing(false); void refresh() } })
  const error = status.error || policy.error || access.error
  return <Card title="共享发送开通与调度政策" extra={<Button onClick={() => { form.resetFields(); form.setFieldsValue({ ...status.data?.policy, expected_revision: status.data?.policy?.revision ?? 0 }); setEditing(true) }}>配置发送政策</Button>}>
    <Alert type="info" title="手机一次启用托管，绑定后自动检查" description="本页面直接使用平台维护的实际应用适配发行。缺少发行或真机验收时，由产品研发处理；普通管理员无需导入 JSON，也不能通过修改状态绕过验收。手机一次明确同意托管范围；绑定和普通适配更新不要求反复授权。" />
    {error && <Alert type="error" title={error.message} />}
    <p>当前政策：{({ disabled: '已禁用', draft_only: '仅草稿', preview_only: '仅预览', test_send: '限定真机测试', production_allowlist: '已验收范围' })[status.data?.policy?.mode ?? 'disabled']}</p>
    <Table rowKey="device_id" dataSource={status.data?.devices ?? []} loading={status.isFetching} pagination={{ pageSize: 10 }} columns={[
      { title: '共享手机', dataIndex: 'name' },
      { title: '实际应用适配', render: (_, d) => !d.environment_fresh ? '等待手机刷新实际环境' : d.matching_releases.length === 1 ? d.matching_releases[0].name : '请产品研发适配或处理发行冲突' },
      { title: '运行与风险', render: (_, d) => d.risk_count ? `${d.risk_count} 项结果待核对，整机阻断` : d.runtime?.quiescent ? '界面静止，可等待新工作' : '等待手机报告静止状态' },
      { title: '验收范围', render: (_, d) => d.acceptance_count ? `${d.acceptance_count} 项有效范围` : '尚无真机验收放行记录' },
      { title: '托管授权', render: (_, d) => d.delegation_accepted ? '本机已同意托管，绑定后自动检查' : '等待手机首次启用托管' },
    ]} />
    <Card title="学校可用设备池" size="small" extra={<Button onClick={()=>{accessForm.resetFields();setAccessOpen(true)}}>批准学校使用手机</Button>}>
      <p>平台一次批准资源，学校管理员可自行绑定本校目标；不扩大手机已同意的能力，不代替真机验收。</p>
      <Table rowKey="id" pagination={{pageSize:10}} dataSource={status.data?.access??[]} columns={[{title:'学校',dataIndex:'school_name'},{title:'手机',dataIndex:'device_name'},{title:'资格',render:(_,r)=>r.enabled?'有效':'已撤销'},{title:'操作',render:(_,r)=><Button disabled={access.isPending} onClick={()=>Modal.confirm({title:r.enabled?'撤销学校设备池资格？':'恢复学校设备池资格？',content:'学校自行建立的相关绑定将重新核验；平台直接配置的具体群绑定独立管理。',onOk:()=>access.mutateAsync({school_id:r.school_id,device_id:r.device_id,enabled:!r.enabled})})}>{r.enabled?'撤销':'恢复'}</Button>}]}/>
    </Card>
    <Modal title="批准学校使用手机" open={accessOpen} confirmLoading={access.isPending} onCancel={()=>setAccessOpen(false)} onOk={()=>void accessForm.validateFields().then(v=>access.mutate(v))}>
      <Form form={accessForm} layout="vertical"><Form.Item name="school_id" label="学校" rules={[{required:true}]}><Select showSearch optionFilterProp="label" options={status.data?.schools?.map(s=>({value:s.id,label:s.name}))}/></Form.Item><Form.Item name="device_id" label="共享手机" rules={[{required:true}]}><Select showSearch optionFilterProp="label" options={status.data?.devices.map(d=>({value:d.device_id,label:d.name}))}/></Form.Item></Form>
    </Modal>
    <Modal title="配置共享发送政策" open={editing} confirmLoading={policy.isPending} onCancel={() => setEditing(false)} onOk={() => void form.validateFields().then(v => policy.mutate(v))}>
      <Form form={form} layout="vertical">
        <Form.Item name="expected_revision" hidden><InputNumber /></Form.Item>
        <Form.Item name="mode" label="放行阶段" rules={[{ required: true }]}><Select options={[['disabled', '禁用'], ['draft_only', '仅草稿'], ['preview_only', '仅预览'], ['test_send', '限定真机测试'], ['production_allowlist', '已验收生产范围']].map(([value, label]) => ({ value, label }))} /></Form.Item>
        <Form.Item name="timezone" label="IANA 时区，例如 Asia/Shanghai" rules={[{ required: true }]}><Input /></Form.Item>
        <p>允许时间采用所选时区，分钟从当天 00:00 起计算；开始大于结束代表跨午夜。</p>
        <Form.List name="allowed_windows">{(fields, { add, remove }) => <>{fields.map(f => <Space key={f.key}>
          <Form.Item name={[f.name, 'weekdays']} rules={[{ required: true }]}><Select mode="multiple" placeholder="星期" style={{ width: 150 }} options={[1, 2, 3, 4, 5, 6, 7].map(value => ({ value, label: `星期${value}` }))} /></Form.Item>
          <Form.Item name={[f.name, 'start_minute']} rules={[{ required: true }]}><InputNumber min={0} max={1439} placeholder="开始分钟" /></Form.Item><Form.Item name={[f.name, 'end_minute']} rules={[{ required: true }]}><InputNumber min={0} max={1440} placeholder="结束分钟" /></Form.Item><Button onClick={() => remove(f.name)}>移除</Button>
        </Space>)}<Button onClick={() => add()}>添加允许时段</Button></>}</Form.List>
        {(['global', 'school', 'device', 'target'] as const).map((dimension, i) => <Card key={dimension} size="small" title={['平台全局', '每所学校', '每台手机', '每个目标群'][i]}>
          {([['interval_seconds', '最小间隔（秒）'], ['window_seconds', '滚动窗口（秒）'], ['max_calls', '窗口最大调用数'], ['daily_max_calls', '每日最大调用数']] as const).map(([key, label]) => <Form.Item key={key} name={['limits', dimension, key]} label={label} rules={[{ required: true }]}><InputNumber min={1} max={key === 'window_seconds' ? 2592000 : 100000} /></Form.Item>)}
        </Card>)}
        <Form.Item name="reason" label="变更理由" rules={[{ required: true }]}><Input.TextArea maxLength={2000} /></Form.Item>
      </Form>
      <p>政策不提供无限量选项。测试与生产仍分别要求有效的设备、学校、目标群和精确环境验收范围。</p>
    </Modal>
  </Card>
}

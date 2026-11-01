import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Col, Drawer, Form, Input, InputNumber, Modal, Row, Select, Space, Table } from 'antd'
import { sharedSend, type SharedPolicyInput } from './api'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { newSharedPolicyLimits } from './shared-policy-defaults'
import { policyWeekdays, policyClock, policyMinute, newPolicyWindow } from './shared-policy-time'

function PolicyTime({ value, onChange, end = false, ...props }: { value?: number; onChange?: (value: number | undefined) => void; end?: boolean; id?: string }) {
  return <Input {...props} type="time" step={60} value={policyClock(value)} onChange={e => onChange?.(policyMinute(e.target.value, end))} />
}

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
  return <Card title="共享发送开通与调度政策" extra={<Button onClick={() => { form.resetFields(); form.setFieldsValue({ timezone: 'Asia/Shanghai', mode: 'disabled', allowed_windows: [], limits: newSharedPolicyLimits(), ...status.data?.policy, expected_revision: status.data?.policy?.revision ?? 0 }); setEditing(true) }}>配置发送政策</Button>}>
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
    <Drawer title="配置共享发送政策" placement="right" size="min(1040px, 100vw)" open={editing} onClose={() => setEditing(false)} footer={<Space><Button onClick={() => setEditing(false)}>取消</Button><Button type="primary" loading={policy.isPending} onClick={() => form.validateFields().then(v => policy.mutate(v)).catch(() => undefined)}>保存政策</Button></Space>}>
      {policy.error && <Alert type="error" title={policy.error.message} />}
      <Form form={form} layout="vertical">
        <Form.Item name="expected_revision" hidden><InputNumber /></Form.Item>
        <Row gutter={16}><Col xs={24} md={12}><Form.Item name="mode" label="放行阶段" rules={[{ required: true }]}><Select options={[['disabled', '禁用'], ['draft_only', '仅草稿'], ['preview_only', '仅预览'], ['test_send', '限定真机测试'], ['production_allowlist', '已验收生产范围']].map(([value, label]) => ({ value, label }))} /></Form.Item></Col>
        <Col xs={24} md={12}><Form.Item name="timezone" label="IANA 时区" rules={[{ required: true }]}><Input placeholder="Asia/Shanghai" /></Form.Item></Col></Row>
        <p>允许时段按所选时区执行，选中的是开始当天。开始晚于结束表示延续至次日；结束 00:00 表示当天 24:00，00:00 至 00:00 表示全天。时段之外不发送。</p>
        <Form.List name="allowed_windows" rules={[{ validator: async (_, rows) => { if (!rows?.length || rows.length > 32) throw new Error('请配置 1 至 32 个允许时段') } }]}>{(fields, { add, remove }, { errors }) => <>
          {fields.map(f => <Card key={f.key} size="small" title={`允许时段 ${f.name + 1}`} extra={<Button onClick={() => remove(f.name)}>移除时段</Button>}>
            <Form.Item name={[f.name, 'weekdays']} label="执行星期" rules={[{ required: true, type: 'array', min: 1, message: '请至少选择一天' }]}><Select mode="multiple" placeholder="选择周一至周日" options={policyWeekdays} /></Form.Item>
            <Space align="start">
              <Form.Item name={[f.name, 'start_minute']} label="开始时间" dependencies={[["allowed_windows", f.name, "end_minute"]]} rules={[{ required: true, message: '请选择开始时间' }, ({getFieldValue})=>({validator:async(_,value)=>{if(value!==undefined && value===getFieldValue(['allowed_windows',f.name,'end_minute']))throw new Error('开始与结束不能相同；全天请选择 00:00 至 00:00')}})]}><PolicyTime /></Form.Item>
              <Form.Item name={[f.name, 'end_minute']} label="结束时间" rules={[{ required: true, message: '请选择结束时间' }]}><PolicyTime end /></Form.Item>
            </Space>
          </Card>)}
          <Form.ErrorList errors={errors} />
          <Button disabled={fields.length >= 32} onClick={() => add(newPolicyWindow())}>添加允许时段</Button>
          <p>新增时段默认为周一至周五 09:00–18:00，可调整后保存；多个时段取并集，未添加时段不能保存。</p>
        </>}</Form.List>
        <p>以下为可调整的初始限额，保存后才生效。四个维度同时约束发送，以最先触达的限制为准；编辑已有政策时保留原值。</p>
        <Row gutter={[16, 12]}>
          {(['global', 'school', 'device', 'target'] as const).map((dimension, i) => <Col key={dimension} xs={24} md={12}>
            <Card size="small" title={['平台全局', '每所学校', '每台手机', '每个目标群'][i]}>
              <Row gutter={12}>
                {([['interval_seconds', '最小间隔（秒）'], ['window_seconds', '滚动窗口（秒）'], ['max_calls', '窗口最大调用数'], ['daily_max_calls', '每日最大调用数']] as const).map(([key, label]) => <Col key={key} span={12}>
                  <Form.Item name={['limits', dimension, key]} label={label} rules={[{ required: true }]} style={{ marginBottom: 12 }}><InputNumber style={{ width: '100%' }} min={1} precision={0} max={key === 'window_seconds' ? 2592000 : key === 'interval_seconds' ? 86400 : 100000} /></Form.Item>
                </Col>)}
              </Row>
            </Card>
          </Col>)}
        </Row>
        <Form.Item name="reason" label="变更理由" rules={[{ required: true }]}><Input.TextArea maxLength={2000} /></Form.Item>
      </Form>
      <p>政策不提供无限量选项。测试与生产仍分别要求有效的设备、学校、目标群和精确环境验收范围。</p>
    </Drawer>
  </Card>
}

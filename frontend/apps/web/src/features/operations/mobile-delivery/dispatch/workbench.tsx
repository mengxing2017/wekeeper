import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Button, Card, Collapse, Input, InputNumber, Select, Space, Table } from 'antd'
import {
  api,
  type DispatchFlowV1,
  type FlowRules,
  type FlowLauncher,
  type FlowVersion,
} from '../api'
import {
  emptyRules,
  emptyLauncher,
  SelectorEditor,
  PageEditor,
  StepEditor,
  policyFields,
  AdaptationWorkbench,
} from '../adaptations'
import { createIdempotencyKey as key } from '../../../../shared/idempotency-key'
import { PaginationControls } from '../../../../shared/pagination-controls'

function RulesEditor({ rules, change }: { rules: FlowRules; change: (v: FlowRules) => void }) {
  const [page, setPage] = useState(''),
    [role, setRole] = useState('')
  return (
    <Space orientation="vertical" className="full-width">
      <PageEditor pages={rules.pages} change={(pages) => change({ ...rules, pages })} />
      <Space>
        <Input
          aria-label="新页面名称"
          placeholder="新增可识别页面"
          value={page}
          onChange={(e) => setPage(e.target.value)}
        />
        <Button
          disabled={!page || !!rules.pages[page]}
          onClick={() => {
            change({
              ...rules,
              pages: {
                ...rules.pages,
                [page]: {
                  marker: { predicate: { visible: true }, ancestor_depth: 1 },
                  required: [],
                  forbidden: [],
                },
              },
            })
            setPage('')
          }}
        >
          添加页面
        </Button>
      </Space>
      <Collapse
        items={Object.entries(rules.controls).map(([name, value]) => ({
          key: name,
          label: `角色：${name}`,
          children: (
            <SelectorEditor
              value={value}
              change={(v) => change({ ...rules, controls: { ...rules.controls, [name]: v } })}
            />
          ),
        }))}
      />
      <Space>
        <Input aria-label="新导航角色" value={role} onChange={(e) => setRole(e.target.value)} />
        <Button
          disabled={!role || !!rules.controls[role]}
          onClick={() => {
            change({
              ...rules,
              controls: {
                ...rules.controls,
                [role]: {
                  predicate: { visible: true, enabled: true, clickable: true },
                  ancestor_depth: 1,
                },
              },
            })
            setRole('')
          }}
        >
          添加导航角色
        </Button>
      </Space>
      <Card title="页面导航图（发送按钮不可作为导航）">
        {rules.navigation_graph.map((edge, i) => (
          <Space key={i} wrap>
            <Select
              aria-label={`导航${i + 1}起点`}
              value={edge.from}
              options={Object.keys(rules.pages).map((value) => ({ value, label: value }))}
              onChange={(from) =>
                change({
                  ...rules,
                  navigation_graph: rules.navigation_graph.map((x, j) =>
                    j === i ? { ...x, from } : x,
                  ),
                })
              }
            />
            <Select
              aria-label={`导航${i + 1}动作`}
              value={edge.control}
              options={[
                { value: '__back', label: '安卓返回' },
                ...Object.keys(rules.controls)
                  .filter((r) => r !== 'send')
                  .map((value) => ({ value, label: `点击 ${value}` })),
              ]}
              onChange={(control) =>
                change({
                  ...rules,
                  navigation_graph: rules.navigation_graph.map((x, j) =>
                    j === i ? { ...x, control } : x,
                  ),
                })
              }
            />
            <Select
              aria-label={`导航${i + 1}终点`}
              value={edge.to}
              options={Object.keys(rules.pages).map((value) => ({ value, label: value }))}
              onChange={(to) =>
                change({
                  ...rules,
                  navigation_graph: rules.navigation_graph.map((x, j) =>
                    j === i ? { ...x, to } : x,
                  ),
                })
              }
            />
            <InputNumber
              aria-label={`导航${i + 1}等待毫秒`}
              min={0}
              value={edge.min_delay_ms}
              onChange={(n) =>
                change({
                  ...rules,
                  navigation_graph: rules.navigation_graph.map((x, j) =>
                    j === i ? { ...x, min_delay_ms: n ?? 0 } : x,
                  ),
                })
              }
            />
            毫秒
            <Button
              onClick={() =>
                change({
                  ...rules,
                  navigation_graph: rules.navigation_graph.filter((_, j) => j !== i),
                })
              }
            >
              删除
            </Button>
          </Space>
        ))}
        <Button
          onClick={() =>
            change({
              ...rules,
              navigation_graph: [
                ...rules.navigation_graph,
                { from: 'entry', to: 'account', control: '__back', min_delay_ms: 0 },
              ],
            })
          }
        >
          添加导航边
        </Button>
      </Card>
      <Space>
        <Input
          aria-label="内部群标志"
          placeholder="内部群可见文字"
          value={rules.group_type_labels.internal}
          onChange={(e) =>
            change({
              ...rules,
              group_type_labels: { ...rules.group_type_labels, internal: e.target.value },
            })
          }
        />
        <Input
          aria-label="外部群标志"
          placeholder="外部群可见文字"
          value={rules.group_type_labels.external}
          onChange={(e) =>
            change({
              ...rules,
              group_type_labels: { ...rules.group_type_labels, external: e.target.value },
            })
          }
        />
        <Select
          aria-label="成员数解析"
          value={rules.member_count_suffix}
          options={[
            { value: 'none', label: '群名不带成员数' },
            { value: 'ascii', label: '群名后 (成员数)' },
            { value: 'full_width', label: '群名后（成员数）' },
          ]}
          onChange={(member_count_suffix) => change({ ...rules, member_count_suffix })}
        />
      </Space>
    </Space>
  )
}
function LauncherEditor({
  flow,
  change,
}: {
  flow: FlowLauncher
  change: (v: FlowLauncher) => void
}) {
  return (
    <Space orientation="vertical" className="full-width">
      <Input
        aria-label="桌面包名"
        placeholder="桌面包名或 *（通用）"
        value={flow.client.package_name}
        onChange={(e) => change({ ...flow, client: { package_name: e.target.value } })}
      />
      <PageEditor pages={flow.pages} change={(pages) => change({ ...flow, pages })} />
      <Collapse
        items={Object.entries(flow.roles).map(([name, value]) => ({
          key: name,
          label: `桌面角色：${name}`,
          children: (
            <SelectorEditor
              value={value}
              change={(v) => change({ ...flow, roles: { ...flow.roles, [name]: v } })}
            />
          ),
        }))}
      />
      <Card title="桌面搜索路径">
        <StepEditor
          launcher
          steps={flow.search_steps}
          pages={Object.keys(flow.pages)}
          roles={Object.keys(flow.roles)}
          change={(search_steps) => change({ ...flow, search_steps })}
        />
      </Card>
      <Card title="全部应用路径">
        <StepEditor
          launcher
          steps={flow.all_apps_steps}
          pages={Object.keys(flow.pages)}
          roles={Object.keys(flow.roles)}
          change={(all_apps_steps) => change({ ...flow, all_apps_steps })}
        />
      </Card>
      {(['max_steps', 'max_scrolls', 'timeout_ms'] as const).map((k) => (
        <label key={k}>
          {
            {
              max_steps: '桌面步骤上限',
              max_scrolls: '桌面滚动上限',
              timeout_ms: '桌面超时（毫秒）',
            }[k]
          }
          <InputNumber min={1} value={flow[k]} onChange={(n) => change({ ...flow, [k]: n ?? 1 })} />
        </label>
      ))}
    </Space>
  )
}
export function DispatchWorkbench() {
  const cache = useQueryClient(),
    lock = useRef(false)
  const [validationDevice, setValidationDevice] = useState<string>(),
    [validationKind, setValidationKind] = useState<
      'measure_manual' | 'open_application' | 'inspect_roles'
    >('measure_manual'),
    [pathKey, setPathKey] = useState('generic'),
    [launcherIndex, setLauncherIndex] = useState(0),
    [manualPackage, setManualPackage] = useState('com.tencent.wework'),
    [manualLabel, setManualLabel] = useState('企业微信')
  const [page, setPage] = useState(1),
    [school, setSchool] = useState(''),
    [pacingId, setPacingId] = useState(''),
    [policyId, setPolicyId] = useState(''),
    [samples, setSamples] = useState<string[]>([]),
    [pacingName, setPacingName] = useState('企业微信群保守人工基准'),
    [job, setJob] = useState(''),
    [publishId, setPublishId] = useState(''),
    [flow, setFlow] = useState<DispatchFlowV1>(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState('')
  const schools = useQuery({
    queryKey: ['mobile', 'platform', 'flow-schools'],
    queryFn: () => api.automation.schools(),
  })
  const versions = useQuery({
    queryKey: ['mobile', 'platform', 'flows', page],
    queryFn: () => api.dispatch.flows(page),
  })
  const pacing = useQuery({
    queryKey: ['mobile', 'platform', 'pacing', page],
    queryFn: () => api.dispatch.pacing(page),
  })
  const components = useQuery({
    queryKey: ['mobile', 'platform', 'flow-components', page],
    queryFn: () => api.automation.components(page),
  })
  const jobs = useQuery({
    queryKey: ['mobile', 'platform', 'flow-validations', school, page],
    queryFn: () => api.automation.validations(school, page),
    enabled: !!school,
  })
  const flowJobs = useQuery({
    queryKey: ['mobile', 'platform', 'v5-validations', school, page],
    queryFn: () => api.dispatch.validations(school, page),
    enabled: !!school,
  })
  const validationDevices = useQuery({
    queryKey: ['mobile', 'platform', 'v5-validation-devices', school, page],
    queryFn: () => api.targets(school, page),
    enabled: !!school,
  })
  const evidenceJobs = [...(flowJobs.data?.items ?? []), ...(jobs.data?.items ?? [])]
  const releases = useQuery({
    queryKey: ['mobile', 'platform', 'flow-releases', page],
    queryFn: () => api.dispatch.releases(undefined, page),
  })
  const failures = useQuery({
    queryKey: ['mobile', 'platform', 'flow-failures', page],
    queryFn: () => api.dispatch.failures(undefined, page),
  })
  async function run(action: () => Promise<void>) {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    try {
      await action()
      await cache.invalidateQueries({ queryKey: ['mobile'] })
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  function initialize() {
    const p = pacing.data?.items.find((v) => v.id === pacingId),
      c = components.data?.items.find((v) => v.id === policyId)
    if (!p || c?.body.kind !== 'policy') return
    setFlow({
      schema_version: 1,
      app_key: 'wecom',
      capability: 'group_text',
      application_label: '企业微信',
      configuration: {
        name: '企业微信群通用流程',
        rules: {
          ...emptyRules({ package_name: 'com.tencent.wework' }),
          schema_version: 2,
          engine_version: 'otter-ui/2',
        },
        policy: structuredClone(c.body.policy),
        baseline: structuredClone(p.baseline),
        evidence_ref: `pacing:${p.id}`,
      },
      launchers: [emptyLauncher({ package_name: '*' })],
      specializations: [],
      max_attempts: 8,
      max_total_steps: 256,
      max_total_scrolls: 64,
      max_total_duration_ms: 300000,
      recoverable_errors: [
        'navigation_failed',
        'control_unavailable',
        'navigation_path_unavailable',
        'application_not_found',
        'unexpected_page',
        'page_changed',
        'page_timeout',
      ],
    })
  }
  function load(v: FlowVersion) {
    setFlow(structuredClone(v.body))
    setPacingId(v.pacing_profile_id)
  }
  return (
    <Space orientation="vertical" className="full-width">
      <Alert
        type="info"
        title="平台通用流程与失败适配"
        description="普通管理员只选流程、设备、群和正文。这里维护非 ID 页面语义、完整桌面路径和保守基准。实际失败才接收版本诊断；未知版本直接使用完整通用路径。"
      />
      {[
        versions,
        pacing,
        components,
        jobs,
        releases,
        failures,
        schools,
        flowJobs,
        validationDevices,
      ].map((q, i) => q.error && <Alert key={i} type="error" title={q.error.message} />)}
      {error && <Alert type="error" title={error} />}
      <Card title="发起真实 v5 维护验证（无需安装快照）">
        <Select
          aria-label="维护设备"
          placeholder="选择试用学校的手机"
          value={validationDevice}
          onChange={setValidationDevice}
          options={validationDevices.data?.devices.map((d) => ({ value: d.id, label: d.name }))}
        />
        <Select
          aria-label="维护验证类型"
          value={validationKind}
          onChange={setValidationKind}
          options={[
            { value: 'measure_manual', label: '真实人工计时' },
            { value: 'inspect_roles', label: '只读页面角色验证' },
            { value: 'open_application', label: '桌面路径打开验证' },
          ]}
        />
        <p>先选择上方试用学校。角色和桌面验证使用下方「待发布流程」，计时也可先于流程创建。</p>
        {validationKind === 'measure_manual' && (
          <Space>
            <Input
              aria-label="计时应用名称"
              value={manualLabel}
              onChange={(e) => setManualLabel(e.target.value)}
            />
            <Input
              aria-label="计时应用包名"
              value={manualPackage}
              onChange={(e) => setManualPackage(e.target.value)}
            />
          </Space>
        )}
        {validationKind === 'inspect_roles' && (
          <Select
            aria-label="待验证分支"
            value={pathKey}
            onChange={setPathKey}
            options={[
              { value: 'generic', label: '完整通用路径' },
              ...(
                versions.data?.items.find((v) => v.id === publishId)?.body.specializations ?? []
              ).map((v) => ({ value: v.key, label: v.key })),
            ]}
          />
        )}
        {validationKind === 'open_application' && (
          <label>
            桌面候选序号（从 0 开始）
            <InputNumber min={0} value={launcherIndex} onChange={(n) => setLauncherIndex(n ?? 0)} />
          </label>
        )}
        <Button
          loading={busy}
          disabled={
            !school || !validationDevice || (validationKind !== 'measure_manual' && !publishId)
          }
          onClick={() =>
            void run(async () => {
              await api.dispatch.validate(
                school,
                {
                  device_id: validationDevice!,
                  flow_id: validationKind === 'measure_manual' ? null : publishId,
                  manual_application:
                    validationKind === 'measure_manual'
                      ? { label: manualLabel, client: { package_name: manualPackage } }
                      : null,
                  kind: validationKind,
                  path_key: validationKind === 'inspect_roles' ? pathKey : 'generic',
                  launcher_index: validationKind === 'open_application' ? launcherIndex : null,
                },
                key(),
              )
            })
          }
        >
          下发到维护手机，等待本机明确接受
        </Button>
        <Table
          rowKey="id"
          pagination={false}
          dataSource={flowJobs.data?.items ?? []}
          columns={[
            { title: '类型', dataIndex: 'kind' },
            { title: '路径', dataIndex: 'path_key' },
            { title: '状态', dataIndex: 'state' },
            {
              title: '取消',
              render: (_, j) => (
                <Button
                  disabled={!['queued', 'accepted'].includes(j.state)}
                  onClick={() =>
                    void run(async () => {
                      await api.dispatch.cancelValidation(school, j, key())
                    })
                  }
                >
                  取消验证
                </Button>
              ),
            },
          ]}
        />
      </Card>
      <Card title="复用真实人工基准">
        <Select
          aria-label="试用学校"
          placeholder="选择有维护权限的试用学校"
          value={school || undefined}
          onChange={setSchool}
          options={schools.data?.map((s) => ({ value: s.id, label: s.name }))}
        />
        <Input
          aria-label="人工基准名称"
          value={pacingName}
          onChange={(e) => setPacingName(e.target.value)}
        />
        <Select
          mode="multiple"
          aria-label="真实人工样本"
          value={samples}
          onChange={setSamples}
          options={evidenceJobs
            .filter((j) => j.kind === 'measure_manual' && j.state === 'succeeded')
            .map((j) => ({
              value: j.id,
              label: `真实人工计时 · ${new Date(j.expires_at).toLocaleString()}`,
            }))}
        />
        <Button
          loading={busy}
          disabled={!samples.length || !school}
          onClick={() =>
            void run(async () => {
              await api.dispatch.createPacing(pacingName, school, samples, key())
            })
          }
        >
          保存可复用基准
        </Button>
      </Card>
      <Card title="新建或复制流程">
        <Select
          aria-label="可复用人工基准"
          placeholder="选择真实基准"
          value={pacingId || undefined}
          onChange={setPacingId}
          options={pacing.data?.items.map((p) => ({ value: p.id, label: p.name }))}
        />
        <Select
          aria-label="保守执行政策"
          placeholder="选择执行政策"
          value={policyId || undefined}
          onChange={setPolicyId}
          options={components.data?.items
            .filter((c) => c.body.kind === 'policy')
            .map((c) => ({ value: c.id, label: c.name }))}
        />
        <Button disabled={!pacingId || !policyId} onClick={initialize}>
          新建通用流程
        </Button>
        <Table
          rowKey="id"
          pagination={false}
          dataSource={versions.data?.items ?? []}
          columns={[
            { title: '流程', dataIndex: 'name' },
            {
              title: '操作',
              render: (_, v) => (
                <Space>
                  <Button onClick={() => load(v)}>复制为新版本</Button>
                  <Button onClick={() => setPublishId(v.id)}>选择发布</Button>
                </Space>
              ),
            },
          ]}
        />
        <PaginationControls
          page={page}
          change={setPage}
          more={versions.data?.page?.has_more ?? false}
        />
      </Card>
      {flow && (
        <Card title="可视化编辑完整流程">
          <Input
            aria-label="通用流程名称"
            value={flow.configuration.name}
            onChange={(e) =>
              setFlow({ ...flow, configuration: { ...flow.configuration, name: e.target.value } })
            }
          />
          <Input
            aria-label="应用名称"
            value={flow.application_label}
            onChange={(e) => setFlow({ ...flow, application_label: e.target.value })}
          />
          <Input
            aria-label="业务应用包名"
            value={flow.configuration.rules.client.package_name}
            onChange={(e) =>
              setFlow({
                ...flow,
                configuration: {
                  ...flow.configuration,
                  rules: { ...flow.configuration.rules, client: { package_name: e.target.value } },
                },
              })
            }
          />
          <Collapse
            items={[
              {
                key: 'generic',
                label: '完整通用业务页面与导航',
                children: (
                  <RulesEditor
                    rules={flow.configuration.rules}
                    change={(rules) =>
                      setFlow({ ...flow, configuration: { ...flow.configuration, rules } })
                    }
                  />
                ),
              },
              {
                key: 'launcher',
                label: '从安卓桌面打开应用',
                children: (
                  <>
                    {flow.launchers.map((l, i) => (
                      <Card key={i}>
                        <LauncherEditor
                          flow={l}
                          change={(v) =>
                            setFlow({
                              ...flow,
                              launchers: flow.launchers.map((x, j) => (i === j ? v : x)),
                            })
                          }
                        />
                        <Button
                          disabled={flow.launchers.length === 1}
                          onClick={() =>
                            setFlow({
                              ...flow,
                              launchers: flow.launchers.filter((_, j) => j !== i),
                            })
                          }
                        >
                          删除桌面候选
                        </Button>
                      </Card>
                    ))}
                    <Button
                      onClick={() =>
                        setFlow({
                          ...flow,
                          launchers: [...flow.launchers, emptyLauncher({ package_name: '' })],
                        })
                      }
                    >
                      添加桌面候选
                    </Button>
                  </>
                ),
              },
              {
                key: 'policy',
                label: '执行节奏与额度',
                children: policyFields.map(([k, label]) => (
                  <label key={k}>
                    {label}
                    <InputNumber
                      min={0}
                      value={flow.configuration.policy[k] / (k.endsWith('_ms') ? 1000 : 1)}
                      onChange={(n) =>
                        setFlow({
                          ...flow,
                          configuration: {
                            ...flow.configuration,
                            policy: {
                              ...flow.configuration.policy,
                              [k]: Math.round((n ?? 0) * (k.endsWith('_ms') ? 1000 : 1)),
                            },
                          },
                        })
                      }
                    />
                  </label>
                )),
              },
              {
                key: 'special',
                label: '可选专项分支（不匹配时使用通用流程）',
                children: (
                  <>
                    {flow.specializations.map((v, i) => (
                      <Card key={i}>
                        <Input
                          aria-label="专项名称"
                          value={v.key}
                          onChange={(e) =>
                            setFlow({
                              ...flow,
                              specializations: flow.specializations.map((x, j) =>
                                j === i ? { ...x, key: e.target.value } : x,
                              ),
                            })
                          }
                        />
                        {(['minimum_version_code', 'maximum_version_code'] as const).map((k) => (
                          <label key={k}>
                            {k === 'minimum_version_code' ? '最低本地版本号' : '最高本地版本号'}
                            <InputNumber
                              min={1}
                              value={v[k]}
                              onChange={(n) =>
                                setFlow({
                                  ...flow,
                                  specializations: flow.specializations.map((x, j) =>
                                    j === i ? { ...x, [k]: n ?? 1 } : x,
                                  ),
                                })
                              }
                            />
                          </label>
                        ))}
                        <RulesEditor
                          rules={v.rules}
                          change={(rules) =>
                            setFlow({
                              ...flow,
                              specializations: flow.specializations.map((x, j) =>
                                j === i ? { ...x, rules } : x,
                              ),
                            })
                          }
                        />
                        <Button
                          onClick={() =>
                            setFlow({
                              ...flow,
                              specializations: flow.specializations.filter((_, j) => j !== i),
                            })
                          }
                        >
                          删除专项
                        </Button>
                      </Card>
                    ))}
                    <Button
                      onClick={() =>
                        setFlow({
                          ...flow,
                          specializations: [
                            ...flow.specializations,
                            {
                              key: `special-${flow.specializations.length + 1}`,
                              minimum_version_code: 1,
                              maximum_version_code: 1,
                              rules: structuredClone(flow.configuration.rules),
                            },
                          ],
                        })
                      }
                    >
                      从通用规则复制专项
                    </Button>
                  </>
                ),
              },
            ]}
          />
          {(
            [
              'max_attempts',
              'max_total_steps',
              'max_total_scrolls',
              'max_total_duration_ms',
            ] as const
          ).map((k) => (
            <label key={k}>
              {
                {
                  max_attempts: '共享尝试次数',
                  max_total_steps: '共享步骤上限',
                  max_total_scrolls: '共享滚动上限',
                  max_total_duration_ms: '共享总时长（毫秒）',
                }[k]
              }
              <InputNumber
                min={1}
                value={flow[k]}
                onChange={(n) => setFlow({ ...flow, [k]: n ?? 1 })}
              />
            </label>
          ))}
          <Select
            mode="multiple"
            aria-label="允许降级的导航错误"
            value={flow.recoverable_errors}
            options={[
              'navigation_failed',
              'control_unavailable',
              'navigation_path_unavailable',
              'application_not_found',
              'unexpected_page',
              'page_changed',
              'page_timeout',
            ].map((value) => ({ value, label: value }))}
            onChange={(recoverable_errors) => setFlow({ ...flow, recoverable_errors })}
          />
          <Button
            type="primary"
            loading={busy}
            onClick={() =>
              void run(async () => {
                const v = await api.dispatch.createFlow(flow, pacingId, key())
                setPublishId(v.id)
              })
            }
          >
            校验并保存不可变版本
          </Button>
        </Card>
      )}
      <Card title="验证发布">
        <p>
          发布需匹配真实页面角色与桌面路径的维护证据；基准、流程及专项证据不按每台业务手机重复采集。
        </p>
        <Select
          aria-label="待发布流程"
          value={publishId || undefined}
          onChange={setPublishId}
          options={versions.data?.items.map((v) => ({ value: v.id, label: v.name }))}
        />
        <Select
          aria-label="真实角色验证记录"
          value={job || undefined}
          onChange={setJob}
          options={evidenceJobs
            .filter((j) => j.kind === 'inspect_roles' && j.state === 'succeeded')
            .map((j) => ({
              value: j.id,
              label: `已成功角色验证 · ${new Date(j.expires_at).toLocaleString()}`,
            }))}
        />
        <Button
          disabled={!publishId || !school || !job}
          loading={busy}
          onClick={() =>
            void run(async () => {
              await api.dispatch.publish(publishId, school, job, key())
            })
          }
        >
          向试用学校发布
        </Button>
        <Table
          rowKey="id"
          pagination={false}
          dataSource={releases.data?.items ?? []}
          columns={[
            { title: '流程', dataIndex: 'name' },
            {
              title: '发布状态',
              render: (_, r) => (r.revoked ? '已撤销' : r.active ? '有效' : '历史'),
            },
            {
              title: '撤销',
              render: (_, r) => (
                <Button
                  disabled={r.revoked}
                  loading={busy}
                  onClick={() =>
                    void run(async () => {
                      await api.dispatch.revoke(r, '平台维护人员撤销流程发布', key())
                    })
                  }
                >
                  撤销并阻止后续执行
                </Button>
              ),
            },
          ]}
        />
      </Card>
      <Card title="执行失败归并">
        <Table
          rowKey="id"
          pagination={false}
          dataSource={failures.data?.items ?? []}
          columns={[
            { title: '失败特征', dataIndex: 'fingerprint' },
            { title: '同类次数', dataIndex: 'occurrences' },
            { title: '步骤', render: (_, f) => `${f.body.path_key} / ${f.body.step}` },
            { title: '原因', render: (_, f) => f.body.reason },
            {
              title: '发送结果',
              render: (_, f) => (f.body.send_state === 'unknown' ? '须人工核对' : '确定未发'),
            },
            {
              title: '诊断版本',
              render: (_, f) =>
                [
                  f.body.versions?.application_name,
                  f.body.versions?.android,
                  f.body.versions?.launcher,
                ]
                  .filter(Boolean)
                  .join(' / ') || '版本不可读，失败已固化',
            },
          ]}
        />
      </Card>
      <Collapse
        items={[
          {
            key: 'maintenance',
            label: '真实维护验证、人工计时及历史 v4 组件',
            children: <AdaptationWorkbench />,
          },
        ]}
      />
    </Space>
  )
}

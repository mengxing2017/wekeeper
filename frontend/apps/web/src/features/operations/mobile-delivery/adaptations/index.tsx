import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import {
  Alert,
  Button,
  Card,
  Checkbox,
  Collapse,
  Input,
  InputNumber,
  Select,
  Space,
  Table,
} from 'antd'
import {
  api,
  type AutomationComponent,
  type AutomationComponentBody,
  type ExecutionPolicy,
  type LauncherFlow,
  type LauncherStep,
  type UiClient,
  type UiRules,
  type UiSelector,
  type ValidationKind,
} from '../api'
import { createIdempotencyKey as key } from '../../../../shared/idempotency-key'
import { PaginationControls } from '../../../../shared/pagination-controls'
import { validationStates } from '../setup'
const selector = (): UiSelector => ({
  predicate: { visible: true, enabled: true },
  ancestor_depth: 1,
  all_of: [],
  any_of: [],
})
const roleNames = [
  'company',
  'account',
  'account_detail',
  'search_input',
  'group_tab',
  'result_title',
  'results_complete',
  'chat_title',
  'group_type',
  'owner',
  'input',
  'send',
  'message_row',
  'message_text',
  'outgoing',
  'failed',
  'pending',
]
const pageNames = ['entry', 'account', 'search', 'results', 'chat']
const roleLabels: Record<string, string> = {
  company: '企业名称',
  account: '账号名称',
  account_detail: '账号详情',
  search_input: '群搜索输入框',
  group_tab: '群分类入口',
  result_title: '搜索结果群名',
  results_complete: '搜索结束标志',
  chat_title: '聊天标题',
  group_type: '群类型',
  owner: '群主',
  input: '消息输入框',
  send: '发送按钮',
  message_row: '消息行',
  message_text: '消息正文',
  outgoing: '本人发送标志',
  failed: '发送失败标志',
  pending: '发送中标志',
  search_entry: '本地应用搜索入口',
  search_field: '本地应用搜索框',
  application_result: '唯一安装应用入口',
  all_apps_entry: '全部应用入口',
  application_list: '应用列表容器',
}
const pageLabels: Record<string, string> = {
  entry: '业务起始页',
  account: '账号页',
  search: '搜索页',
  results: '结果页',
  chat: '聊天页',
  home: '安卓桌面',
  all_apps: '全部应用页',
  application: '目标应用核验页',
}
const policyFields: Array<[keyof ExecutionPolicy, string]> = [
  ['account_cooldown_ms', '账号冷却（秒）'],
  ['content_cooldown_ms', '同文冷却（秒）'],
  ['default_group_interval_ms', '建议群间等待（秒）'],
  ['minimum_group_interval_ms', '最低群间等待（秒）'],
  ['session_max_duration_ms', '最长会话（秒）'],
  ['quota_window_ms', '额度统计窗口（秒）'],
  ['account_limit', '账号额度'],
  ['group_limit', '单群额度'],
  ['max_targets', '最多目标群'],
  ['input_codepoint_delay_ms', '每字输入间隔（秒）'],
  ['input_settle_ms', '输入后稳定等待（秒）'],
  ['page_timeout_ms', '页面超时（秒）'],
  ['observation_timeout_ms', '发送观察超时（秒）'],
  ['poll_interval_ms', '观察轮询间隔（秒）'],
  ['max_navigation_steps', '最多导航步骤'],
  ['minimum_manual_samples', '至少人工样本数'],
]
function SelectorEditor({ value, change }: { value: UiSelector; change: (v: UiSelector) => void }) {
  function predicate(section: 'predicate' | 'parent' | 'ancestor') {
    const p = value[section] ?? {}
    return (
      <Space wrap>
        {(['text', 'description', 'class_name'] as const).map((k) => (
          <Input
            key={k}
            aria-label={`${section}-${k}`}
            placeholder={
              { text: '准确可见文字', description: '无障碍描述', class_name: '节点类名' }[k]
            }
            value={p[k] ?? ''}
            onChange={(e) => change({ ...value, [section]: { ...p, [k]: e.target.value || null } })}
          />
        ))}
        {(['visible', 'enabled', 'editable', 'clickable'] as const).map((k) => (
          <Select
            key={k}
            aria-label={`${section}-${k}`}
            style={{ minWidth: 145 }}
            value={p[k] == null ? 'any' : String(p[k])}
            options={[
              {
                value: 'any',
                label: `${{ visible: '可见', enabled: '可用', editable: '可编辑', clickable: '可点击' }[k]}：不限`,
              },
              { value: 'true', label: `${k}：是` },
              { value: 'false', label: `${k}：否` },
            ]}
            onChange={(v) =>
              change({ ...value, [section]: { ...p, [k]: v === 'any' ? null : v === 'true' } })
            }
          />
        ))}
      </Space>
    )
  }
  return (
    <Space orientation="vertical" className="full-width">
      {predicate('predicate')}
      <Collapse
        items={[
          {
            key: 'structure',
            label: '父节点与祖先条件（可选）',
            children: (
              <Space orientation="vertical">
                <Checkbox
                  checked={!!value.parent}
                  onChange={(e) =>
                    change({ ...value, parent: e.target.checked ? { visible: true } : null })
                  }
                >
                  限定父节点
                </Checkbox>
                {value.parent && predicate('parent')}
                <Checkbox
                  checked={!!value.ancestor}
                  onChange={(e) =>
                    change({
                      ...value,
                      ancestor: e.target.checked ? { visible: true } : null,
                      ancestor_depth: 1,
                    })
                  }
                >
                  限定祖先节点
                </Checkbox>
                {value.ancestor && (
                  <>
                    {predicate('ancestor')}
                    <InputNumber
                      aria-label="祖先查找层数"
                      min={1}
                      max={16}
                      value={value.ancestor_depth}
                      onChange={(n) => change({ ...value, ancestor_depth: n ?? 1 })}
                    />
                  </>
                )}
              </Space>
            ),
          },
        ]}
      />
    </Space>
  )
}
function PageEditor({
  pages,
  change,
}: {
  pages: UiRules['pages']
  change: (p: UiRules['pages']) => void
}) {
  return (
    <Collapse
      items={Object.entries(pages).map(([name, p]) => ({
        key: name,
        label: `${pageLabels[name] || name}：页面标志`,
        children: (
          <Space orientation="vertical" className="full-width">
            <SelectorEditor
              value={p.marker}
              change={(marker) => change({ ...pages, [name]: { ...p, marker } })}
            />
            {(['required', 'forbidden'] as const).map((kind) => (
              <div key={kind}>
                <p>{kind === 'required' ? '必须同时出现' : '出现即阻断'}的条件</p>
                {p[kind].map((s, i) => (
                  <Card key={i}>
                    <SelectorEditor
                      value={s}
                      change={(v) =>
                        change({
                          ...pages,
                          [name]: { ...p, [kind]: p[kind].map((x, j) => (j === i ? v : x)) },
                        })
                      }
                    />
                    <Button
                      onClick={() =>
                        change({
                          ...pages,
                          [name]: { ...p, [kind]: p[kind].filter((_, j) => i !== j) },
                        })
                      }
                    >
                      删除条件
                    </Button>
                  </Card>
                ))}
                <Button
                  onClick={() =>
                    change({ ...pages, [name]: { ...p, [kind]: [...p[kind], selector()] } })
                  }
                >
                  添加{kind === 'required' ? '必需' : '阻断'}条件
                </Button>
              </div>
            ))}
          </Space>
        ),
      }))}
    />
  )
}
function StepEditor({
  steps,
  pages,
  roles,
  launcher,
  change,
}: {
  steps: LauncherStep[]
  pages: string[]
  roles: string[]
  launcher: boolean
  change: (s: LauncherStep[]) => void
}) {
  const actions = launcher
    ? [
        { value: 'home', label: '回到主屏幕' },
        { value: 'click', label: '点击唯一角色' },
        { value: 'search_application', label: '输入绑定应用名称' },
        { value: 'scroll_forward', label: '语义向前滚动' },
        { value: 'verify_application', label: '核验目标应用前台' },
      ]
    : [{ value: 'click', label: '点击唯一导航角色' }]
  return (
    <Space orientation="vertical" className="full-width">
      {steps.map((s, i) => (
        <Space key={i} wrap>
          <span>步骤 {i + 1}</span>
          <Select
            aria-label={`步骤${i + 1}起点`}
            value={s.from}
            options={pages.map((p) => ({ value: p, label: pageLabels[p] || p }))}
            onChange={(from) => change(steps.map((x, j) => (j === i ? { ...x, from } : x)))}
          />
          <Select
            aria-label={`步骤${i + 1}动作`}
            value={s.operation.action}
            options={actions}
            onChange={(action) =>
              change(
                steps.map((x, j) =>
                  j === i
                    ? {
                        ...x,
                        operation:
                          action === 'home' || action === 'verify_application'
                            ? { action }
                            : {
                                action: action as 'click' | 'search_application' | 'scroll_forward',
                                role: roles[0] ?? '',
                              },
                      }
                    : x,
                ),
              )
            }
          />
          {'role' in s.operation && (
            <Select
              aria-label={`步骤${i + 1}角色`}
              value={s.operation.role}
              options={roles.map((r) => ({ value: r, label: roleLabels[r] || r }))}
              onChange={(role) =>
                change(
                  steps.map((x, j) =>
                    j === i
                      ? { ...x, operation: { action: s.operation.action as 'click', role } }
                      : x,
                  ),
                )
              }
            />
          )}
          <Select
            aria-label={`步骤${i + 1}终点`}
            value={s.to}
            options={pages.map((p) => ({ value: p, label: pageLabels[p] || p }))}
            onChange={(to) => change(steps.map((x, j) => (j === i ? { ...x, to } : x)))}
          />
          <InputNumber
            aria-label={`步骤${i + 1}等待秒数`}
            min={0}
            precision={3}
            value={s.delay_ms / 1000}
            onChange={(n) =>
              change(
                steps.map((x, j) =>
                  j === i ? { ...x, delay_ms: Math.round((n ?? 0) * 1000) } : x,
                ),
              )
            }
          />
          秒<Button onClick={() => change(steps.filter((_, j) => i !== j))}>删除步骤</Button>
        </Space>
      ))}
      <Button
        onClick={() =>
          change([
            ...steps,
            {
              from: steps.at(-1)?.to ?? pages[0],
              to: pages[0],
              operation: launcher ? { action: 'home' } : { action: 'click', role: roles[0] ?? '' },
              delay_ms: 0,
            },
          ])
        }
      >
        添加步骤
      </Button>
    </Space>
  )
}
function emptyRules(client: UiClient): UiRules {
  return {
    schema_version: 1,
    engine_version: 'otter-ui/1',
    required_capabilities: [
      'non_id_query/1',
      'bounded_navigation/1',
      'visible_verification/1',
      'ordered_groups/1',
    ],
    client,
    pages: Object.fromEntries(
      pageNames.map((p) => [p, { marker: selector(), required: [], forbidden: [] }]),
    ),
    controls: Object.fromEntries(
      roleNames.map((r) => [
        r,
        {
          ...selector(),
          predicate: {
            ...selector().predicate,
            ...(['input', 'search_input'].includes(r)
              ? { editable: true }
              : r === 'send'
                ? { clickable: true }
                : {}),
          },
        },
      ]),
    ),
    navigation_graph: [],
    landmarks: {
      entry: 'entry',
      account: 'account',
      search: 'search',
      results: 'results',
      chat: 'chat',
    },
    group_type_labels: { internal: '', external: '' },
    member_count_suffix: 'none',
  }
}
function emptyLauncher(client: UiClient): LauncherFlow {
  return {
    client,
    pages: Object.fromEntries(
      ['home', 'search', 'all_apps', 'application'].map((p) => [
        p,
        { marker: selector(), required: [], forbidden: [] },
      ]),
    ),
    roles: Object.fromEntries(
      [
        'search_entry',
        'search_field',
        'application_result',
        'all_apps_entry',
        'application_list',
      ].map((r) => [
        r,
        {
          ...selector(),
          predicate: {
            ...selector().predicate,
            ...(r === 'search_field' ? { editable: true } : { clickable: true }),
          },
        },
      ]),
    ),
    search_steps: [],
    all_apps_steps: [],
    max_steps: 32,
    max_scrolls: 16,
    timeout_ms: 60000,
    local_application_results_only: true,
  }
}
const kindLabels = {
  launcher: '安卓桌面流程',
  workflow: '企业微信群业务页面',
  policy: '执行政策',
  baseline: '人工计时基准',
}
export function AdaptationWorkbench() {
  const [school, setSchool] = useState(''),
    [bindingId, setBindingId] = useState(''),
    [page, setPage] = useState(1),
    [bindingPage, setBindingPage] = useState(1),
    [jobPage, setJobPage] = useState(1),
    [releasePage, setReleasePage] = useState(1),
    [name, setName] = useState(''),
    [body, setBody] = useState<AutomationComponentBody>(),
    [component, setComponent] = useState<AutomationComponent>(),
    [validationId, setValidationId] = useState(''),
    [policy, setPolicy] = useState<Partial<ExecutionPolicy>>({}),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [reason, setReason] = useState(''),
    [scope, setScope] = useState<'device' | 'school'>('device'),
    [newRole, setNewRole] = useState(''),
    [newPage, setNewPage] = useState(''),
    [supportingWorkflow, setSupportingWorkflow] = useState('')
  const schools = useQuery({
    queryKey: ['mobile', 'adaptation-schools'],
    queryFn: () => api.automation.schools(),
  })
  const bindings = useQuery({
    queryKey: ['mobile', school, 'app-bindings', bindingPage],
    queryFn: () => api.automation.bindings(school, bindingPage),
    enabled: !!school,
  })
  const binding = bindings.data?.items.find((b) => b.id === bindingId)
  const environment = useQuery({
    queryKey: ['mobile', school, 'adaptation-environment', binding?.device_id],
    queryFn: () => api.automation.applications(school, binding!.device_id),
    enabled: !!binding,
  })
  const versions = useQuery({
    queryKey: ['mobile', 'platform', 'automation-components', page],
    queryFn: () => api.automation.components(page),
  })
  const jobs = useQuery({
    queryKey: ['mobile', school, 'adaptation-validations', jobPage],
    queryFn: () => api.automation.validations(school, jobPage),
    enabled: !!school,
    refetchInterval: school ? 5000 : false,
  })
  const releases = useQuery({
    queryKey: ['mobile', 'platform', 'automation-releases', releasePage],
    queryFn: () => api.automation.releases(releasePage),
  })
  async function perform(action: () => Promise<void>) {
    if (busy) return
    setBusy(true)
    setError('')
    try {
      await action()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }
  function start(kind: AutomationComponentBody['kind']) {
    setComponent(undefined)
    setName('')
    setPolicy({})
    setValidationId('')
    const report = environment.data?.environment?.body
    const client = report?.applications.find(
      (a) => a.instance_ref === binding?.instance_ref,
    )?.client
    if (kind === 'baseline') setBody({ kind, validation_ids: [] })
    else if (kind === 'policy') setBody({ kind, policy: {} as ExecutionPolicy })
    else if (kind === 'workflow' && client) setBody({ kind, rules: emptyRules(client) })
    else if (kind === 'launcher' && report) setBody({ kind, flow: emptyLauncher(report.launcher) })
  }
  async function validate(kind: ValidationKind) {
    if (!binding) return
    await api.automation.validate(
      school,
      {
        device_id: binding.device_id,
        app_binding_id: binding.id,
        kind,
        component_ids: component
          ? [
              component.id,
              ...(component.kind === 'policy' && supportingWorkflow ? [supportingWorkflow] : []),
            ]
          : [],
      },
      key(),
    )
    await jobs.refetch()
  }
  const saved =
    !!component &&
    name === component.name &&
    JSON.stringify(body) === JSON.stringify(component.body)
  const completed =
    jobs.data?.items.filter((j) => j.state === 'succeeded' && j.app_binding_id === binding?.id) ??
    []
  return (
    <Space orientation="vertical" className="full-width">
      <Alert
        type="info"
        title="适配维护：编辑 → 手机验证 → 审核发布"
        description="使用真实环境的可见条件与人工计时；平台维护身份不代替学校试用授权。没有新版手机或真实样本时保留待适配，不生成可用默认规则。"
      />
      {[
        schools.error,
        bindings.error,
        environment.error,
        versions.error,
        jobs.error,
        releases.error,
      ].map((e, i) => e && <Alert key={i} type="error" title={e.message} />)}
      {error && <Alert type="error" title={error} />}
      <Card title="选择试用范围">
        <Space wrap>
          <Select
            aria-label="适配学校"
            placeholder="选择有管理权限的学校"
            style={{ minWidth: 220 }}
            value={school || undefined}
            options={schools.data?.map((s) => ({ value: s.id, label: s.name }))}
            onChange={(v) => {
              setSchool(v)
              setBindingId('')
              setValidationId('')
              setBody(undefined)
            }}
          />
          <Select
            aria-label="适配手机应用"
            placeholder="选择已设置应用的试用手机"
            style={{ minWidth: 320 }}
            value={bindingId || undefined}
            options={bindings.data?.items.map((b) => ({
              value: b.id,
              label: `${b.device_name} · ${b.app_name} · ${b.account_name}`,
            }))}
            onChange={(v) => {
              setBindingId(v)
              setValidationId('')
              setBody(undefined)
            }}
          />
        </Space>
        <PaginationControls
          page={bindingPage}
          more={bindings.data?.page?.has_more}
          change={setBindingPage}
          subject="适配手机"
        />
        {binding && !environment.data?.environment && (
          <Alert type="warning" title="等待新版手机端上报环境" />
        )}
      </Card>
      <Card title="不可变版本">
        <Space wrap>
          {(Object.keys(kindLabels) as AutomationComponentBody['kind'][]).map((kind) => (
            <Button
              key={kind}
              disabled={!environment.data?.environment}
              onClick={() => start(kind)}
            >
              新建{kindLabels[kind]}
            </Button>
          ))}
        </Space>
        <Table
          rowKey="id"
          pagination={false}
          dataSource={versions.data?.items ?? []}
          columns={[
            { title: '名称', dataIndex: 'name' },
            { title: '种类', render: (_, v) => kindLabels[v.kind] },
            {
              title: '操作',
              render: (_, v) => (
                <Space>
                  <Button
                    onClick={() => {
                      setComponent(v)
                      setBody(v.body)
                      setName(v.name)
                      if (v.body.kind === 'policy') setPolicy(v.body.policy)
                    }}
                  >
                    选择或复制
                  </Button>
                </Space>
              ),
            },
          ]}
        />
        <PaginationControls
          page={page}
          more={versions.data?.page?.has_more}
          change={setPage}
          subject="适配版本"
        />
      </Card>
      {body && (
        <Card title={`${kindLabels[body.kind]}：可视化编辑`}>
          <Space orientation="vertical" className="full-width">
            <Input
              aria-label="适配版本名称"
              placeholder="便于识别的名称"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            {(body.kind === 'workflow' || body.kind === 'launcher') && (
              <>
                <Space>
                  <Input
                    aria-label="新增页面名称"
                    placeholder="新增中间页或异常页名称"
                    value={newPage}
                    onChange={(e) => setNewPage(e.target.value)}
                  />
                  <Button
                    disabled={
                      !newPage.trim() ||
                      !!(body.kind === 'workflow' ? body.rules.pages : body.flow.pages)[
                        newPage.trim()
                      ]
                    }
                    onClick={() => {
                      const pages = {
                        ...(body.kind === 'workflow' ? body.rules.pages : body.flow.pages),
                        [newPage.trim()]: { marker: selector(), required: [], forbidden: [] },
                      }
                      setBody(
                        body.kind === 'workflow'
                          ? { ...body, rules: { ...body.rules, pages } }
                          : { ...body, flow: { ...body.flow, pages } },
                      )
                      setNewPage('')
                    }}
                  >
                    添加页面
                  </Button>
                </Space>
                <PageEditor
                  pages={body.kind === 'workflow' ? body.rules.pages : body.flow.pages}
                  change={(pages) =>
                    setBody(
                      body.kind === 'workflow'
                        ? { ...body, rules: { ...body.rules, pages } }
                        : { ...body, flow: { ...body.flow, pages } },
                    )
                  }
                />
                <Collapse
                  items={Object.entries(
                    body.kind === 'workflow' ? body.rules.controls : body.flow.roles,
                  ).map(([role, s]) => ({
                    key: role,
                    label: roleLabels[role] || role,
                    children: (
                      <SelectorEditor
                        value={s}
                        change={(v) =>
                          setBody(
                            body.kind === 'workflow'
                              ? {
                                  ...body,
                                  rules: {
                                    ...body.rules,
                                    controls: { ...body.rules.controls, [role]: v },
                                  },
                                }
                              : {
                                  ...body,
                                  flow: { ...body.flow, roles: { ...body.flow.roles, [role]: v } },
                                },
                          )
                        }
                      />
                    ),
                  }))}
                />
              </>
            )}
            {body.kind === 'workflow' && (
              <>
                <Space>
                  <Input
                    aria-label="新导航角色名称"
                    placeholder="新增返回或导航角色名称"
                    value={newRole}
                    onChange={(e) => setNewRole(e.target.value)}
                  />
                  <Button
                    disabled={!newRole.trim() || !!body.rules.controls[newRole]}
                    onClick={() => {
                      setBody({
                        ...body,
                        rules: {
                          ...body.rules,
                          controls: { ...body.rules.controls, [newRole.trim()]: selector() },
                        },
                      })
                      setNewRole('')
                    }}
                  >
                    添加导航角色
                  </Button>
                </Space>
                <StepEditor
                  launcher={false}
                  pages={Object.keys(body.rules.pages)}
                  roles={Object.keys(body.rules.controls).filter(
                    (r) => r !== 'send' && r !== 'input',
                  )}
                  steps={body.rules.navigation_graph.map((e) => ({
                    from: e.from,
                    to: e.to,
                    operation: { action: 'click', role: e.control },
                    delay_ms: e.min_delay_ms,
                  }))}
                  change={(steps) =>
                    setBody({
                      ...body,
                      rules: {
                        ...body.rules,
                        navigation_graph: steps.map((s) => ({
                          from: s.from,
                          to: s.to,
                          control: 'role' in s.operation ? s.operation.role : '',
                          min_delay_ms: s.delay_ms,
                        })),
                      },
                    })
                  }
                />
                <Space>
                  {(['internal', 'external'] as const).map((k) => (
                    <Input
                      key={k}
                      aria-label={`${k}群可见标志`}
                      placeholder={`${k === 'internal' ? '内部' : '外部'}群类型可见文字`}
                      value={body.rules.group_type_labels[k]}
                      onChange={(e) =>
                        setBody({
                          ...body,
                          rules: {
                            ...body.rules,
                            group_type_labels: {
                              ...body.rules.group_type_labels,
                              [k]: e.target.value,
                            },
                          },
                        })
                      }
                    />
                  ))}
                  <Select
                    aria-label="群名成员数格式"
                    value={body.rules.member_count_suffix}
                    options={[
                      { value: 'none', label: '标题不带成员数' },
                      { value: 'ascii', label: '群名后带 (成员数)' },
                      { value: 'full_width', label: '群名后带（成员数）' },
                    ]}
                    onChange={(v) =>
                      setBody({ ...body, rules: { ...body.rules, member_count_suffix: v } })
                    }
                  />
                </Space>
              </>
            )}
            {body.kind === 'launcher' && (
              <>
                <p>
                  未配置的路径不会启用。每条路径必须从主屏幕动作开始，以目标应用核验结束；不能通过
                  Intent 或坐标操作替代。
                </p>
                {(['search_steps', 'all_apps_steps'] as const).map((path) => (
                  <Card
                    key={path}
                    title={path === 'search_steps' ? '本地搜索路径' : '全部应用路径'}
                  >
                    <StepEditor
                      launcher
                      pages={Object.keys(body.flow.pages)}
                      roles={Object.keys(body.flow.roles)}
                      steps={body.flow[path]}
                      change={(steps) =>
                        setBody({ ...body, flow: { ...body.flow, [path]: steps } })
                      }
                    />
                  </Card>
                ))}
                <Space>
                  {(['max_steps', 'max_scrolls', 'timeout_ms'] as const).map((k) => (
                    <label key={k}>
                      {
                        {
                          max_steps: '最多步骤',
                          max_scrolls: '最多滚动次数',
                          timeout_ms: '总超时（秒）',
                        }[k]
                      }
                      <InputNumber
                        min={1}
                        value={body.flow[k] / (k === 'timeout_ms' ? 1000 : 1)}
                        onChange={(v) =>
                          setBody({
                            ...body,
                            flow: { ...body.flow, [k]: (v ?? 0) * (k === 'timeout_ms' ? 1000 : 1) },
                          })
                        }
                      />
                    </label>
                  ))}
                </Space>
              </>
            )}
            {body.kind === 'policy' && (
              <Space wrap>
                {policyFields.map(([k, label]) => (
                  <label key={k}>
                    {label}
                    <InputNumber
                      aria-label={label}
                      min={0}
                      precision={k.endsWith('_ms') ? 3 : 0}
                      value={
                        policy[k] === undefined ? null : policy[k]! / (k.endsWith('_ms') ? 1000 : 1)
                      }
                      onChange={(v) => {
                        const p = {
                          ...policy,
                          [k]:
                            v === null ? undefined : Math.round(v * (k.endsWith('_ms') ? 1000 : 1)),
                        }
                        setPolicy(p)
                        setBody({ kind: 'policy', policy: p as ExecutionPolicy })
                      }}
                    />
                  </label>
                ))}
              </Space>
            )}
            {body.kind === 'baseline' && (
              <>
                <p>
                  基准由手机端记录的真实人工操作边界生成，取所选有效样本的保守最大值。不能填一个样本数冒充采集。
                </p>
                <Select
                  mode="multiple"
                  aria-label="人工计时样本"
                  className="full-width"
                  value={body.validation_ids}
                  options={completed
                    .filter((j) => j.kind === 'measure_manual')
                    .map((j, i) => ({
                      value: j.id,
                      label: `人工样本 ${i + 1} · 群间 ${Math.ceil((j.result?.timing?.group_interval_ms ?? 0) / 1000)} 秒`,
                    }))}
                  onChange={(validation_ids) => setBody({ kind: 'baseline', validation_ids })}
                />
              </>
            )}
            <Button
              type="primary"
              loading={busy}
              disabled={!name.trim()}
              onClick={() =>
                void perform(async () => {
                  const v = await api.automation.createComponent({ name, body }, key())
                  setComponent(v)
                  await versions.refetch()
                })
              }
            >
              校验并保存新版本（保存不发布）
            </Button>
          </Space>
        </Card>
      )}
      <Card title="手机验证与审核发布">
        <p>
          当前验证和发布版本：{component?.name ?? '请先保存或选择版本'}
          。表单修改后需保存为新版本，再重新验证。
        </p>
        <Space orientation="vertical" className="full-width">
          <p>
            正常手机仅可测试打开；角色采集和人工计时需要手机维护模式。所有任务须手机显式接受，Web
            不自动操纵界面。
          </p>
          <Select
            aria-label="政策验证业务版本"
            placeholder="政策验证时选择对应业务页面版本"
            style={{ minWidth: 300 }}
            value={supportingWorkflow || undefined}
            options={versions.data?.items
              .filter((v) => v.kind === 'workflow')
              .map((v) => ({ value: v.id, label: v.name }))}
            onChange={setSupportingWorkflow}
          />
          <Space>
            {(
              [
                {
                  kind: 'open_application',
                  label: '试用桌面打开',
                  allow: component?.kind === 'launcher',
                },
                {
                  kind: 'inspect_roles',
                  label: '只读角色匹配',
                  allow:
                    component?.kind === 'workflow' ||
                    (component?.kind === 'policy' && !!supportingWorkflow),
                },
                { kind: 'measure_manual', label: '记录人工计时', allow: true },
              ] as const
            ).map((v) => (
              <Button
                key={v.kind}
                loading={busy}
                disabled={!binding || !v.allow || (v.kind !== 'measure_manual' && !saved)}
                onClick={() => void perform(() => validate(v.kind))}
              >
                {v.label}
              </Button>
            ))}
          </Space>
          <Table
            rowKey="id"
            pagination={false}
            dataSource={jobs.data?.items ?? []}
            columns={[
              {
                title: '验证种类',
                render: (_, j) =>
                  ({
                    open_application: '测试打开',
                    inspect_environment: '环境检查',
                    inspect_roles: '角色匹配',
                    measure_manual: '人工计时',
                  })[j.kind],
              },
              { title: '状态', render: (_, j) => validationStates[j.state] },
              {
                title: '操作',
                render: (_, j) =>
                  ['queued', 'offered', 'accepted', 'running'].includes(j.state) && (
                    <Button
                      onClick={() =>
                        void perform(async () => {
                          await api.automation.cancel(school, j.id, j.revision, key())
                          await jobs.refetch()
                        })
                      }
                    >
                      取消验证
                    </Button>
                  ),
              },
            ]}
          />
          <PaginationControls
            page={jobPage}
            more={jobs.data?.page?.has_more}
            change={setJobPage}
            subject="适配验证"
          />
          <Select
            aria-label="发布验证依据"
            placeholder="选择覆盖本版本的成功验证"
            style={{ minWidth: 320 }}
            value={validationId || undefined}
            options={completed.map((j, i) => ({
              value: j.id,
              label: `成功验证 ${i + 1} · ${j.kind === 'open_application' ? '测试打开' : j.kind === 'measure_manual' ? '人工计时' : '角色匹配'}${j.result?.path ? ` · ${j.result.path === 'search' ? '搜索' : '全部应用'}` : ''}`,
            }))}
            onChange={setValidationId}
          />
          <Select
            aria-label="发布适用范围"
            value={scope}
            options={[
              { value: 'device', label: '仅试用手机' },
              { value: 'school', label: '当前学校精确环境' },
            ]}
            onChange={setScope}
          />
          <Button
            type="primary"
            loading={busy}
            disabled={!saved || !binding || !validationId}
            onClick={() =>
              void perform(async () => {
                if (!component || !binding) return
                await api.automation.publish(
                  component,
                  {
                    school_id: school,
                    device_id: scope === 'device' ? binding.device_id : null,
                    validation_id: validationId,
                  },
                  key(),
                )
                await releases.refetch()
              })
            }
          >
            审核通过并发布所选版本
          </Button>
        </Space>
      </Card>
      <Card title="发布记录与撤销">
        <Input
          aria-label="撤销适配原因"
          placeholder="撤销原因"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
        <Table
          rowKey="id"
          pagination={false}
          dataSource={releases.data?.items ?? []}
          columns={[
            { title: '组件', render: (_, r) => kindLabels[r.kind] },
            {
              title: '状态',
              render: (_, r) => (r.revoked ? '已撤销' : r.active ? '生效' : '已被替代'),
            },
            {
              title: '操作',
              render: (_, r) =>
                !r.revoked && (
                  <Button
                    disabled={!reason.trim() || busy}
                    onClick={() =>
                      void perform(async () => {
                        await api.automation.revoke(r, reason, key())
                        await releases.refetch()
                      })
                    }
                  >
                    撤销
                  </Button>
                ),
            },
          ]}
        />
        <PaginationControls
          page={releasePage}
          more={releases.data?.page?.has_more}
          change={setReleasePage}
          subject="适配发布"
        />
      </Card>
    </Space>
  )
}

import {BusinessRequests} from '../wecom-senders/business-requests'
import { createIdempotencyKey } from '../../../shared/idempotency-key'
import { useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Alert,
  Button,
  Card,
  Checkbox,
  Descriptions,
  Drawer,
  Table,
  Input,
  Select,
  Space,
  Tag,
} from 'antd'
import { Field } from '../../../shared/field'
import { PaginationControls } from '../../../shared/pagination-controls'
import { useDialogs } from '../../../shared/use-dialogs'
import { formatDate } from '../../../shared/format-date'
import {
  broadcast,
  executionEvidenceLabels,
  workspace,
  product,
  type BroadcastTask,
  type BroadcastTarget,
  type BroadcastDraftInput,
  type BroadcastAction,
  type DistributionMaterial,
  type Row,
} from './api'

const creation: Record<string, string> = {
  queued: '待提交',
  submitting: '创建中',
  created: '已创建，等待成员执行',
  rejected: '创建被拒绝',
  unknown: '创建结果不确定',
  cancelled: '已取消',
}
export function targetLabel(t: BroadcastTarget) {
  if (t.official_status === 1) return '官方报告已发送（非已读）'
  if (t.rejected) return '平台拒绝此目标'
  if (t.official_status === 2) return '平台报告非好友失败（需核对）'
  if (t.official_status === 3) return '平台报告群发接收额度限制'
  if (t.official_status !== null && t.official_status !== 0)
    return `待核验状态：${t.official_status}`
  if (t.stop_state === 'stopped') return '已停止后续发送；未取得本群发送证据'
  return creation[t.creation_state] ?? '未取得结果'
}
const taskLabel = (t: BroadcastTask) =>
  t.state === 'draft'
    ? '草稿'
    : t.state === 'abandoned'
      ? '已放弃'
      : t.uncertain
        ? '结果待核对'
        : t.sent === t.total && t.total > 0
          ? '全部已发送'
          : t.stopped
            ? '已请求停止'
            : t.rejected
              ? '有拒绝结果'
              : t.sent
                ? '部分已发送'
                : '待创建 / 待成员执行'
export default function BroadcastTasks({
  school,
  admin,
  platformAdmin = false,
}: {
  school: string
  admin: boolean
  platformAdmin?: boolean
}) {
  const [selected, setSelected] = useState(new URLSearchParams(window.location.search).get('task')??''),
    [page, setPage] = useState(1),
    [status, setStatus] = useState('')
  const [channelReason,setChannelReason]=useState(''),[channelBusy,setChannelBusy]=useState(false),[channelError,setChannelError]=useState('')
  const [editing, setEditing] = useState<{ id?: string; input?: BroadcastDraftInput } | null>(null)
  const query = useQuery({
    queryKey: ['business', school, 'broadcast', page, status],
    queryFn: () => broadcast.list(school, page, status),
  })
  const capability = useQuery({
    queryKey: ['business', school, 'broadcast-capability'],
    queryFn: () => broadcast.capability(school),
  })
  const cache = useQueryClient()
  const refresh = async () => {
    await cache.invalidateQueries({ queryKey: ['business', school] })
  }
  return (
    <section>
      <Button href={`/admin?view=source-automation-tasks&channel=wecom&workspace_id=${encodeURIComponent(school)}`}>自动链路执行与证据</Button>
      <Alert type="info" title="企业微信分发管理 · 官方接口" description="客户群、模板、官方发送身份和结果属于官方渠道；失败不会自动改用手机。" />
      {capability.data && !capability.data.broadcast_available && (
        <Alert type="warning" title={capability.data.reason} />
      )}
      {capability.data&&!capability.data.broadcast_available&&<Space orientation="vertical"><p>负责人：{capability.data.request?.owner_name??'待平台分派'} · {capability.data.request?.response??'可保留草稿；开通后仍需本人确认'}</p><Input aria-label="渠道开通用途" placeholder="填写开通用途或当前问题" value={channelReason} onChange={e=>setChannelReason(e.target.value)}/><Button loading={channelBusy} disabled={!channelReason.trim()} onClick={()=>{setChannelBusy(true);setChannelError('');void product.requests.channel(school,{reason:channelReason}).then(()=>{setChannelReason('');return refresh()}).catch(e=>setChannelError(e instanceof Error?e.message:'提交失败')).finally(()=>setChannelBusy(false))}}>确认提交开通申请</Button></Space>}
      {channelError&&<Alert type="error" title={channelError}/>}
      {capability.data?.execution&&<Alert type="info" title="官方客户群通道：成员在企业微信客户端确认属于外部流程；来源自动任务无需平台逐条确认。创建任务不等于逐群发送成功。"/>}
      {capability.error && <Alert type="error" title={capability.error.message} />}
          <div className="query-filter">
            <Space wrap>
              <Select
                aria-label="任务状态"
                value={status}
                onChange={(v) => {
                  setStatus(v)
                  setPage(1)
                }}
                options={[
                  { value: '', label: '全部任务' },
                  { value: 'draft', label: '草稿' },
                  { value: 'confirmed', label: '已确认' },
                  { value: 'pending', label: '待执行' },
                  { value: 'issues', label: '异常' },
                  { value: 'abandoned', label: '已放弃' },
                ]}
              />
              <Button onClick={() => void refresh()}>刷新列表</Button>
            </Space>
          </div>
          {editing && (
            <Drawer
              open
              size="large"
              title={editing.id ? '编辑群发草稿' : '新建群发任务'}
              onClose={() => setEditing(null)}
            >
              <DraftEditor
                school={school}
                admin={admin}
                initial={editing.input}
                id={editing.id}
                saved={async (t) => {
                  setEditing(null)
                  setSelected(t.id)
                  await refresh()
                }}
                cancel={() => setEditing(null)}
              />
            </Drawer>
          )}
          {selected && (
            <Drawer open size="large" title="群发任务详情" onClose={() => setSelected('')}>
              <Detail
                key={selected}
                school={school}
                id={selected}
                admin={admin}
                platformAdmin={platformAdmin}
                enabled={capability.data?.broadcast_available ?? false}
                unavailableReason={
                  capability.error
                    ? '群发能力读取失败，请刷新页面重试。'
                    : (capability.data?.reason ?? '正在检查群发能力…')
                }
                managementAvailable={capability.data?.management_available ?? false}
                refresh={refresh}
                edit={(id, input) => {
                  setEditing({ id, input })
                  setSelected('')
                }}
              />
            </Drawer>
          )}
          {query.error && <Alert type="error" title={query.error.message} />}
          {query.isPending && <p role="status">正在加载任务…</p>}
          <Card
            className="data-panel"
            title="群发任务列表"
            extra={
              <Button
                type="primary"
                onClick={() => {
                  setEditing({})
                  setSelected('')
                }}
              >
                新建群发任务
              </Button>
            }
          >
            <Table<BroadcastTask>
              rowKey="id"
              loading={query.isFetching}
              dataSource={query.data?.items ?? []}
              pagination={false}
              scroll={{ x: 1000 }}
              locale={{ emptyText: '暂无群发任务' }}
              columns={[
                { title: '消息内容', dataIndex: 'content', ellipsis: true, width: 260 },
                {
                  title: '任务状态',
                  render: (_, t) => (
                    <Tag
                      color={
                        t.uncertain ? 'warning' : t.state === 'draft' ? 'default' : 'processing'
                      }
                    >
                      {taskLabel(t)}
                    </Tag>
                  ),
                },
                {
                  title: '内容来源',
                  render: (_, t) => (t.content_source === 'text' ? '直接填写' : t.content_source === 'materials' ? '独立来源素材' : '论坛内容'),
                },
                {
                  title: '目标群 / 批次',
                  render: (_, t) => `${t.total} 群 / ${t.batch_count} 批次`,
                },
                {
                  title: '已发送 / 拒绝 / 不确定',
                  render: (_, t) => `${t.sent} / ${t.rejected} / ${t.uncertain}`,
                },
                { title: '创建时间', render: (_, t) => formatDate(t.created_at), width: 180 },
                {
                  title: '操作',
                  fixed: 'right',
                  width: 160,
                  render: (_, t) => (
                    <Button
                      type="link"
                      onClick={() => {
                        setSelected(t.id)
                        setEditing(null)
                      }}
                    >
                      查看任务 {t.id.slice(-6)}
                    </Button>
                  ),
                },
              ]}
            />
          </Card>
          <PaginationControls
            page={page}
            more={query.data?.page?.has_more}
            busy={query.isFetching}
            change={setPage}
          />
    </section>
  )
}
function DraftEditor({
  school,
  admin,
  initial,
  id,
  saved,
  cancel,
}: {
  admin: boolean
  school: string
  initial?: BroadcastDraftInput
  id?: string
  saved: (task: BroadcastTask) => Promise<void>
  cancel: () => void
}) {
  const [currentId,setCurrentId]=useState(id),[currentRevision,setCurrentRevision]=useState(initial?.revision)
  const [result,setResult]=useState<Awaited<ReturnType<typeof broadcast.savePreview>>>(),[previewTargets,setPreviewTargets]=useState<BroadcastTarget[]>([])
  const confirmKey=useRef(createIdempotencyKey())
  const [source, setSource] = useState<'posts' | 'text' | 'materials'>(
    initial ? (initial.content_source ?? 'posts') : 'text',
  )
  const [text, setText] = useState(initial?.content_source === 'text' ? initial.text_content : '')
  const dialogs = useDialogs()
  const normalized = text.replace(/\r\n?/g, '\n'),
    bytes = new TextEncoder().encode(normalized).length
  const textValid =
    normalized.trim().length > 0 &&
    bytes <= 4000 &&
    !Array.from(normalized).some((c) => {
      const n = c.codePointAt(0)!
      return (n < 32 || (n >= 127 && n <= 159)) && c !== '\n' && c !== '\t'
    })
  const [chosen, setChosen] = useState(initial?.post_ids ?? []),
    [groups, setGroups] = useState(initial?.group_ids ?? []),
    [version, setVersion] = useState(initial?.template_version_id ?? '')
  const [search, setSearch] = useState(''),
    [page, setPage] = useState(1),
    [reason, setReason] = useState(initial?.reason ?? '')
  const [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  const lock = useRef(false)
  const saveIntent = useRef<{ payload: string; key: string } | null>(null)
  const posts = useQuery({
    enabled: source === 'posts',
    queryKey: ['business', school, 'broadcast-posts', search, page],
    queryFn: () =>
      workspace.list<DistributionMaterial>(
        school,
        'distribution-materials',
        `?page[number]=${page}&page[size]=30&search=${encodeURIComponent(search)}`,
      ),
  })
  const targets = useQuery({
    queryKey: ['business', school, 'groups'],
    queryFn: () => workspace.list(school, 'groups'),
  })
  const templates = useQuery({
    enabled: source === 'posts',
    queryKey: ['business', school, 'templates'],
    queryFn: () => workspace.list(school, 'templates'),
  })
  const versions =
    templates.data?.items.flatMap((t) =>
      Array.isArray(t.versions)
        ? (t.versions as Row[])
            .filter((v) => v.published && v.id===t.current_version_id)
            .map((v) => ({ value: v.id, label: `${String(t.name)} · ${formatDate(String(v.published_at))}` }))
        : [],
    ) ?? []
  async function save() {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    try {
      const common = {
        group_ids: groups,
        reason,
        revision: currentRevision,
        source_task_id: initial?.source_task_id,
      }
      const input: BroadcastDraftInput =
        source === 'text'
          ? { ...common, content_source: 'text', text_content: normalized }
          : source === 'materials' && initial?.content_source === 'materials' ? { ...common, content_source: 'materials', materials: initial.materials }
          : { ...common, content_source: 'posts', template_version_id: version, post_ids: chosen }
      const payload = JSON.stringify(input)
      if (saveIntent.current?.payload !== payload) saveIntent.current = { payload, key: createIdempotencyKey() }
      const response=await broadcast.savePreview(school,input,currentId,saveIntent.current.key)
      setResult(response);setCurrentId(response.task_id)
      const savedTask=await broadcast.task(school,response.task_id);setCurrentRevision(savedTask.revision)
      if(response.preview)setPreviewTargets(await allTargets(school,response.task_id))
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  return (
    <Card title={id ? '修改任务草稿' : '创建任务草稿'}>
      {dialogs.contextHolder}
      <p>预览会自动保留草稿，不创建企业微信任务。正文上限 4000 UTF-8 字节。</p>
      {error && <Alert type="error" title={error} />}
      {!result&&<>
      <Field label="内容来源">
        <Select
          aria-label="内容来源"
          value={source}
          disabled={busy}
          options={[
            { value: 'text', label: '直接填写' },
            { value: 'posts', label: '从来源素材生成' },
          ]}
          onChange={(v) =>
            void (async () => {
              if (
                v !== source &&
                (await dialogs.confirm('切换内容来源将清空当前正文、帖子及模板选择，是否继续？'))
              ) {
                setSource(v)
                setText('')
                setChosen([])
                setVersion('')
              }
            })()
          }
        />
      </Field>
      {source === 'materials' && <Alert type="info" title="独立来源素材" description="修改目标后需要重新预览；素材内容与版本由受控来源提供。" />}
      {source === 'text' && (
        <>
          <Field label="群发正文">
            <Input.TextArea
              aria-label="群发正文"
              rows={6}
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="直接输入通知，无需社区帖子或模板"
            />
          </Field>
          <p>{bytes} / 4000 UTF-8 字节 · 直接文本需当前工作空间管理员确认提交，运营人员可保存草稿。</p>
          <pre className="message-preview" aria-label="正文预览">
            {normalized}
          </pre>
          {!!text && !textValid && (
            <Alert
              type="error"
              title="正文须为非空文本，不超过 4000 UTF-8 字节，且不能包含不支持的控制字符"
            />
          )}
        </>
      )}
      {(source === 'posts' ? [posts, targets, templates] : [targets]).map(
        (q, i) => q.error && <Alert key={i} type="error" title={q.error.message} />,
      )}
      {source === 'posts' && (
        <>
          <Field label="搜索已审核内容">
            <Input
              value={search}
              onChange={(e) => {
                setSearch(e.target.value)
                setPage(1)
              }}
            />
          </Field>
          <p>
            已选 {chosen.length} 条<Button onClick={() => setChosen([])}>清空内容</Button>
          </p>
          {posts.data?.items
            .map((p) => (
              <Checkbox
                className="check-row"
                key={p.id}
                checked={chosen.includes(p.id)}
                onChange={(e) =>
                  setChosen(
                    e.target.checked ? [...chosen, p.id] : chosen.filter((id) => id !== p.id),
                  )
                }
              >
                {p.title}
              </Checkbox>
            ))}
          <PaginationControls
            page={page}
            more={posts.data?.page?.has_more}
            busy={posts.isFetching}
            change={setPage}
            subject="内容"
          />
          <Field label="已发布模板">
            <Select
              aria-label="已发布模板"
              className="full-width"
              value={version || undefined}
              options={versions}
              onChange={setVersion}
            />
          </Field>
        </>
      )}
      <Field label="目标客户群">
        <Select
          aria-label="目标客户群"
          className="full-width"
          mode="multiple"
          value={groups}
          options={targets.data?.items
            .filter((g) => g.active)
            .map((g) => ({ value: g.id, label: String(g.name) }))}
          onChange={setGroups}
        />
      </Field>
      {targets.isSuccess && !targets.data.items.some((g) => g.active) && (
        <p>暂无可用客户群，可以先保留正文草稿；平台准备好当前工作空间范围后再预览。</p>
      )}
      <p>
        预览会核验目录和群主分发工作空间授权；未就绪时保留草稿，确认前再次校验。
      </p>
      {initial?.source_task_id && (
        <Field label="再次发送原因">
          <Input.TextArea
            value={reason}
            maxLength={1000}
            onChange={(e) => setReason(e.target.value)}
          />
        </Field>
      )}

      <Space>
        <Button
          type="primary"
          loading={busy}
          disabled={(source === 'text' ? !textValid : source === 'materials' ? initial?.content_source !== 'materials' : !chosen.length || !version)}
          onClick={() => void save()}
        >
          预览
        </Button>
        <Button disabled={busy} onClick={cancel}>
          关闭
        </Button>
      </Space>
      </>}
      {result&&<>
       {result.preview?<><h3>完整发送预览</h3><pre>{result.preview.content}</pre><p>全部 {result.preview.target_count} 个群；确认仅创建官方任务，执行成员仍需在企业微信确认。</p><ul>{previewTargets.map(v=><li key={v.id}>{v.group_name} · 执行成员：{v.sender}</li>)}</ul>
       <Button disabled={busy} onClick={()=>{setResult(undefined);saveIntent.current=null}}>返回修改</Button>
       <Button type="primary" disabled={busy||(source==='text'&&!admin)||previewTargets.length!==result.preview.target_count} onClick={()=>{if(lock.current)return;lock.current=true;setBusy(true);setError('');void broadcast.action(school,result.task_id,'confirm',confirmKey.current,{revision:result.preview!.revision,preview_digest:result.preview!.digest}).then(saved).catch(e=>setError(e instanceof Error?e.message:'确认失败')).finally(()=>{lock.current=false;setBusy(false)})}}>确认创建官方群发任务</Button>
       {source==='text'&&!admin&&<p>请当前工作空间管理员从同一草稿核对确认；当前已保存正文。</p>}</>:<Alert type="info" title={result.reason??'草稿已保存'}/>}
       <Button disabled={busy} onClick={()=>void broadcast.task(school,result.task_id).then(saved)}>查看已保存草稿</Button>
      </>}
    </Card>
  )
}
function taskContent(t: BroadcastTask) {
  if (t.content_source === 'materials') return { content_source: 'materials' as const, materials: t.materials }
  return t.content_source === 'text'
    ? { content_source: 'text' as const, text_content: t.content }
    : {
        content_source: 'posts' as const,
        template_version_id: t.template_version_id!,
        post_ids: t.post_ids,
      }
}
async function allTargets(s: string, id: string) {
  const rows: BroadcastTarget[] = []
  for (let page = 1; page <= 100; page++) {
    const result = await broadcast.targets(s, id, page)
    rows.push(...result.items)
    if (!result.page?.has_more) return rows
  }
  throw new Error('目标数量超过支持范围')
}
function Detail({
  school,
  id,
  admin,
  enabled,
  unavailableReason,
  managementAvailable,
  refresh,
  edit,
}: {
  school: string
  id: string
  admin: boolean
  platformAdmin: boolean
  enabled: boolean
  unavailableReason: string
  managementAvailable: boolean
  refresh: () => Promise<void>
  edit: (id: string | undefined, input: BroadcastDraftInput) => void
}) {
  const [page, setPage] = useState(1),
    [eventPage, setEventPage] = useState(1),
    [observationPage,setObservationPage] = useState(1),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  const [batch, setBatch] = useState(''),
    [decision, setDecision] = useState<BroadcastAction['decision']>('not_created'),
    [reason, setReason] = useState(''),
    [evidenceId, setEvidenceId] = useState('')
  const [candidates,setCandidates]=useState<Awaited<ReturnType<typeof broadcast.evidenceCandidates>>['items']>([])
  const [evidenceReason,setEvidenceReason]=useState(''),[issueReason,setIssueReason]=useState('')
  const keys = useRef(new Map<string, string>()),
    lock = useRef(false),
    dialogs = useDialogs(),
    cache = useQueryClient()
  const task = useQuery({
    queryKey: ['business', school, 'broadcast-task', id],
    queryFn: () => broadcast.task(school, id),
    refetchInterval: 15000,
  })
  const targets = useQuery({
    queryKey: ['business', school, 'broadcast-targets', id, page],
    queryFn: () => broadcast.targets(school, id, page),
    refetchInterval: 15000,
  })
  const events = useQuery({
    queryKey: ['business', school, 'broadcast-events', id, eventPage],
    queryFn: () => broadcast.events(school, id, eventPage),
  })
  const observations = useQuery({queryKey:['business',school,'broadcast-observations',id,observationPage],queryFn:()=>broadcast.observations(school,id,observationPage),refetchInterval:15000})
  async function run(job: () => Promise<unknown>) {
    if (lock.current) return
    lock.current = true
    setBusy(true)
    setError('')
    try {
      await job()
      await refresh()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      lock.current = false
      setBusy(false)
    }
  }
  async function act(
    operation: Parameters<typeof broadcast.action>[2],
    input: BroadcastAction = {},
  ) {
    const signature = JSON.stringify([operation, input])
    let key = keys.current.get(signature)
    if (!key) {
      key = createIdempotencyKey()
      keys.current.set(signature, key)
    }
    const updated = await broadcast.action(school, id, operation, key, input)
    keys.current.delete(signature)
    cache.setQueryData(['business', school, 'broadcast-task', id], updated)
  }
  const t = task.data
  if (!t)
    return (
      <Alert type={task.error ? 'error' : 'info'} title={task.error?.message ?? '正在读取任务…'} />
    )
  return (
    <Card title={<>{taskLabel(t)} · 任务详情</>}>
      {dialogs.contextHolder}
      {t.state === 'draft' && (
        <Alert
          type={enabled && (t.content_source !== 'text' || admin) ? 'info' : 'warning'}
          title={
            !enabled
              ? unavailableReason
              : t.content_source === 'text' && !admin
                ? '当前账号没有当前工作空间管理员权限，不能提交直接文本群发。请当前工作空间管理员打开此草稿确认提交；系统管理员身份不能替代分发工作空间授权。'
                : '草稿尚未提交到企业微信。请检查下方正文和目标群，再点击“确认创建企业微信群发任务”。'
          }
        />
      )}
      {t.state === 'confirmed' && (
        <Alert
          type="info"
          title="已确认提交，后台正在创建或跟踪企业微信任务。取得官方任务 ID 后，请执行成员到企业微信群发助手确认发送；逐群结果见下方。"
        />
      )}
      <Tag>{t.content_source === 'text' ? '直接填写' : t.content_source === 'materials' ? '独立来源素材' : '论坛内容'}</Tag>
      {t.state === 'draft' && t.content_source === 'text' && !admin && (
        <p>直接文本草稿需当前工作空间管理员检查并确认提交。</p>
      )}
      <Descriptions
        bordered
        column={2}
        items={[
          { key: 'id', label: '任务 ID', children: t.id, span: 2 },
          { key: 'groups', label: '目标群', children: t.total },
          { key: 'batches', label: '群主批次', children: t.batch_count },
          {
            key: 'bytes',
            label: '内容长度',
            children: `${new TextEncoder().encode(t.content).length} 字节`,
          },
          { key: 'created', label: '创建时间', children: formatDate(t.created_at) },
        ]}
      />
      <section className="detail-section">
        <h3>消息正文</h3>
        <pre className="message-preview">{t.content}</pre>
      </section>
      <p>
        目标清单分页展示；确认将涵盖全部 {t.total} 个群。执行成员须使用企业微信 4.1.10 或更新版本。
      </p>
      {error && <Alert type="error" title={error} />}
      <Space wrap>
        {t.state === 'draft' && (
          <>
            <Button
              type="primary"
              loading={busy}
              disabled={!enabled || busy || (t.content_source === 'text' && !admin)}
              onClick={() =>
                void run(async () => {
                  const preview = await broadcast.preview(school, t.id)
                  if (preview.revision !== t.revision || preview.content !== t.content || preview.target_count !== t.total) throw new Error('任务内容或目标已变化，请刷新后重新预览')
                  await act('confirm', { revision: preview.revision, preview_digest: preview.digest })
                })
              }
            >
              确认创建企业微信群发任务
            </Button>
            <Button
              disabled={busy}
              onClick={() =>
                void run(async () =>
                  edit(id, {
                    ...taskContent(t),
                    group_ids: t.draft_input.group_ids,
                    revision: t.revision,
                    source_task_id: t.source_task_id ?? undefined,
                    reason: t.resend_reason,
                  }),
                )
              }
            >
              修改草稿
            </Button>
            <Button disabled={busy} onClick={() => void run(() => act('abandon'))}>
              放弃草稿
            </Button>
          </>
        )}
        {t.state === 'confirmed' && (
          <>
            <Button disabled={busy} onClick={() => void run(() => act('refresh'))}>
              同步平台结果
            </Button>
            <Button
              disabled={!enabled || busy || t.stopped}
              onClick={() =>
                void run(async () => {
                  if (await dialogs.confirm('提醒未执行成员完成群发任务？')) await act('reminders')
                })
              }
            >
              提醒成员
            </Button>
            {admin && (
              <>
                <Button
                  danger
                  disabled={!managementAvailable || busy}
                  onClick={() =>
                    void run(async () => {
                      if (await dialogs.confirm('停止尚未执行的任务？已经发送的消息无法撤回。'))
                        await act('stop')
                    })
                  }
                >
                  停止后续发送
                </Button>
                <Button
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      const reason = await dialogs.prompt('再次发送原因（先停止并核对原任务）')
                      if (!reason) return
                      const rows = (await allTargets(school, id)).filter(
                        (x) => x.official_status !== 1,
                      )
                      edit(undefined, {
                        ...taskContent(t),
                        group_ids: rows.map((x) => x.group_id),
                        source_task_id: id,
                        reason,
                      })
                    })
                  }
                >
                  生成再次发送草稿
                </Button>
              </>
            )}
          </>
        )}
      </Space>
      {targets.error && <Alert type="error" title={targets.error.message} />}
      {targets.data?.items.map((x) => (
        <Card size="small" key={x.id} title={`${x.group_name} · ${x.sender}`}>
          <p>{x.chat_id}</p>
          {x.msgid && <p>已关联官方任务，可同步成员及目标群结果。</p>}
          <p>{t.state === 'draft' ? '已选为目标群，等待确认提交' : targetLabel(x)}</p>
          {t.state === 'confirmed' && x.msgid && (
            <p>
              成员任务：
              {x.member_status === 2
                ? '成员已执行（不代表所有群成功）'
                : x.member_status === 0
                  ? '成员未执行'
                  : '未取得成员结果'}{' '}
              · 最后同步：{x.last_synced_at ? formatDate(x.last_synced_at) : '尚未同步'}
            </p>
          )}
          {x.evidence&&<p>证据：{executionEvidenceLabels[x.evidence.evidence_type]??x.evidence.evidence_type} · 观察时间：{formatDate(x.evidence.observed_at)}</p>}
          {!!x.sync_failures&&<p>同步连续失败 {x.sync_failures} 次；下次自动查询：{x.next_sync_at?formatDate(x.next_sync_at):'待安排'}</p>}
          {x.sync_error && <p>同步未完成，保留上次结果</p>}
          {x.error_code && <p>创建原因：{x.error_code}</p>}
          {x.stop_state !== 'none' && <p>停止状态：{({none:'无停止请求',requested:'已申请停止',stopped:'已停止',unknown:'停止结果待核对',rejected:'停止未受理'}[x.stop_state])}</p>}
          {x.reconciliation && <p>人工核对：{({not_created:'有证据确认未创建',sent:'人工确认已发送',linked:'已关联官方证据',stop_confirmed:'已确认停止',reminder_reviewed:'已核对提醒'}[x.reconciliation]??'已登记核对依据')}（与平台事实独立）</p>}
          {admin &&
            (x.creation_state === 'unknown' ||
              x.stop_state === 'unknown' ||
              x.operation_unknown) && (
              <Button onClick={() => {setBatch(x.batch_id);setEvidenceId('');setCandidates([])}}>核对此批次</Button>
            )}
        </Card>
      ))}
      <PaginationControls
        page={page}
        more={targets.data?.page?.has_more}
        busy={targets.isFetching}
        change={setPage}
        subject="目标群"
      />
      <Card size="small" title="执行观察记录">
       {observations.error&&<Alert type="error" title={observations.error.message}/>}
       {observations.data?.items.map(o=><p key={o.id}>{executionEvidenceLabels[o.evidence_type]??o.evidence_type} · {formatDate(o.observed_at)} · {o.target_id??o.batch_id??'平台任务'}</p>)}
       <PaginationControls page={observationPage} more={observations.data?.page?.has_more} busy={observations.isFetching} change={setObservationPage} subject="执行证据"/>
      </Card>
      {admin && batch && (
        <Card title="核对未决操作">
          <p>没有查到记录不能证明未创建。请结合企业微信管理端证据，未核对前禁止再次发送。</p>
          <Select
            aria-label="核对结论"
            value={decision}
            options={[
              { value: 'not_created', label: '有证据确认未创建' },
              { value: 'sent', label: '人工确认已发送（仍保留未知平台状态）' },
              { value: 'linked', label: '选择证据关联官方任务' },
              { value: 'stop_confirmed', label: '有证据确认停止已生效' },
              { value: 'reminder_reviewed', label: '提醒已核对（不重新发送）' },
            ]}
            onChange={setDecision}
          />
          {decision==='linked'&&<Select aria-label="候选证据" placeholder="先查询当前工作空间可见证据" value={evidenceId||undefined} options={candidates.map(v=>({value:v.id,label:`${v.summary.content.slice(0,60)} · ${v.summary.sender_name??v.summary.sender} · ${v.summary.target_count} 个群 · ${v.summary.created_at?formatDate(new Date(v.summary.created_at*1000).toISOString()):'时间待核对'}`}))} onChange={setEvidenceId}/>}
          <Input.TextArea
            aria-label="核对依据"
            placeholder="核对依据"
            maxLength={1000}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
          <Button
            disabled={busy || !reason.trim() || (decision==='linked'&&!evidenceId)}
            onClick={() =>
              void run(() =>
                act('reconciliations', {
                  batch_id: batch,
                  decision,
                  reason,
                  ...(decision === 'linked' ? {evidence_id:evidenceId} : {}),
                  revision:t.revision,
                }),
              )
            }
          >
            保存核对依据
          </Button>
          <Button disabled={busy} onClick={()=>void run(async()=>{const response=await broadcast.evidenceCandidates(school,id,batch);setCandidates(response.items);setEvidenceReason(response.reason);setEvidenceId('')})}>查询当前工作空间候选证据</Button>
          {evidenceReason&&<p>{evidenceReason}</p>}

        </Card>
      )}
      <Card title="报告群发问题"><Input.TextArea aria-label="问题现象" placeholder="描述现象，系统自动关联当前工作空间任务与范围" value={issueReason} onChange={e=>setIssueReason(e.target.value)}/><Button disabled={busy||!issueReason.trim()} onClick={()=>void run(async()=>{await product.requests.issue(school,{reason:issueReason,task_id:id,batch_id:batch||undefined});setIssueReason('')})}>确认移交负责人</Button></Card>
      <BusinessRequests school={school}/>
      <h3>操作记录</h3>
      {events.error && <Alert type="error" title={events.error.message} />}
      {events.data?.items.map((e) => (
        <p key={e.id}>
          {formatDate(e.created_at)} · {e.actor_name} · {e.description} {e.reason}
        </p>
      ))}
      <PaginationControls
        page={eventPage}
        more={events.data?.page?.has_more}
        busy={events.isFetching}
        change={setEventPage}
        subject="操作记录"
      />
    </Card>
  )
}

import { PaginationControls } from '../../../shared/pagination-controls'
import { Field } from '../../../shared/field'
import { Disclosure } from '../../../shared/disclosure'
import { Alert, Button, Input, Select, Steps, Tag , Empty , Card  } from 'antd'
import { useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { identity, workspace, wecomOAuthUrl, type Draft, type Attempt } from './api';
import { currentChat, sendText, sendOutcome } from './sdk';

function messageOf(error: unknown) {
    if (error instanceof Error) return error.message;
    return '客户端操作未完成，请检查网络与应用权限后重新核对';
}
const stateNames: Record<string, string> = {
    prepared: '等待客户端结果', unknown: '结果不确定，待核对', cancelled: '已取消',
    client_reported_success: '客户端报告成功', client_reported_failure: '客户端报告失败',
    operator_confirmed: '人工确认已发送', operator_rejected: '人工确认未发送',
};
export function ShareWorkspace() {
    const status = useQuery({ queryKey: ['wecom-status'], queryFn: workspace.wecomStatus, retry: false });
    const session = useQuery({ queryKey: ['session'], queryFn: identity.session, retry: false });
    const params = new URLSearchParams(location.search);
    const [school, setSchool] = useState(params.get('school') ?? '');
    const [selected, setSelected] = useState(params.get('draft') ?? '');
    const user = session.data?.user;
    const isWecom = /wxwork/i.test(navigator.userAgent);
    const schools = useQuery({ queryKey: ['schools'], queryFn: workspace.schools, enabled: !!user, retry: false });
    const permittedSchools = schools.data?.filter(item => user?.platform_admin || user?.grants.some(grant => grant.school_id === item.id && ['operator', 'admin'].includes(grant.role))) ?? [];
    const s = permittedSchools.find(item => item.id === school)?.id || (!school ? permittedSchools[0]?.id : '') || '';
    const [draftPage, setDraftPage] = useState(1), [historyPage, setHistoryPage] = useState(1), [relatedPage, setRelatedPage] = useState(1);
    const drafts = useQuery({ queryKey: ['business', s, 'delivery-drafts', draftPage], queryFn: () => workspace.list<Draft>(s, 'delivery-drafts', `?page[number]=${draftPage}&page[size]=30`), enabled: !!s, retry: false });
    const history = useQuery({ queryKey: ['business', s, 'delivery-attempts', historyPage], queryFn: () => workspace.list<Attempt>(s, 'delivery-attempts', `?page[number]=${historyPage}&page[size]=30`), enabled: !!s, retry: false, refetchInterval: 30000 });
    const detail = useQuery({ queryKey: ['business', s, 'delivery-draft', selected], queryFn: () => workspace.getDraft(s, selected), enabled: !!s && !!selected, retry: false });
    const related = useQuery({ queryKey: ['business', s, 'related-history', selected, relatedPage], queryFn: () => workspace.relatedHistory(s, selected, relatedPage), enabled: !!s && !!selected, retry: false, refetchInterval: 30000 });
    const draft = detail.data;
    const draftOptions = drafts.data ? [...drafts.data.items] : [];
    if (draft && !draftOptions.some(d => d.id === draft.id)) draftOptions.unshift(draft);
    const [chat, setChat] = useState(''), [error, setError] = useState(''), [message, setMessage] = useState(''), [busy, setBusy] = useState(false), [checking, setChecking] = useState(false), [resend, setResend] = useState(''), [reason, setReason] = useState('');
    const lock = useRef(false);
    const attempted = useRef(new Set<string>());
    const canSend = !!user?.wecom_authenticated && isWecom && !!status.data?.share_available;
    const canResend = !!user?.platform_admin || !!user?.grants.some(g => g.school_id === s && g.role === 'admin');
    const oauthUrl = wecomOAuthUrl({ school: s || params.get('school') || undefined, draft: selected || undefined });
    async function check() {
        if (!canSend || checking || lock.current) return;
        setChecking(true); setChat(''); setError('');
        try { setChat(await currentChat()); } catch (e) { setError(messageOf(e)); }
        finally { setChecking(false); }
    }
    async function share() {
        if (!draft || !canSend || lock.current || checking || attempted.current.has(draft.id)) return;
        lock.current = true; setBusy(true); setError(''); setMessage('');
        let attempt: Attempt | undefined;
        try {
            const actual = await currentChat();
            if (actual !== chat || actual !== draft.chat_id) throw new Error('当前群与草稿目标不一致，请重新核对');
            const storageKey = `wekeeper.attempt:${user!.id}:${s}:${draft.id}`;
            let key = sessionStorage.getItem(storageKey);
            if (!key) { key = crypto.randomUUID(); sessionStorage.setItem(storageKey, key); }
            attempt = await workspace.prepare(s, draft.id, key, { current_chat_id: actual, resend_of: resend || undefined, reason });
            attempted.current.add(draft.id);
            if (attempt.state !== 'prepared') throw new Error('该操作已有结果，请先在分享记录核对');
            const invokedKey = `wekeeper.invoked:${attempt.id}`;
            if (localStorage.getItem(invokedKey)) throw new Error('此尝试已在客户端执行过，请人工核对，不能重复调用发送');
            localStorage.setItem(invokedKey, 'true');
            await workspace.claim(s, attempt.id);
            const state = await sendOutcome(() => sendText(draft.content));
            await workspace.result(s, attempt.id, state);
            setMessage(state === 'client_reported_success' ? '客户端报告调用成功，请在目标群核对实际消息。' : '此次操作未确认成功，请查看分享记录并人工核对。');
        } catch (e) { setError(`${messageOf(e)}${attempt ? '；本次不会自动重发。' : ''}`); }
        finally { setBusy(false); lock.current = false; void history.refetch(); void related.refetch(); }
    }
    const sessionError = session.error && !('status' in session.error && session.error.status === 401);
    return <section className="wecom-workspace">
        <div className="wecom-title"><Tag color="cyan" className="workspace-badge">{canSend ? '企业成员工作空间' : '预览与接入'}</Tag><h1>企业微信人工分享</h1><p>选择学校与草稿，核对当前客户群，再人工确认分享。</p></div>
        <Steps className="wecom-steps" size="small" current={!user ? 0 : !draft ? 1 : 2} items={[{ title: '确认身份' }, { title: '预览正文' }, { title: '核对并分享' }]} />
        {params.has('oauth_error') && <Alert type="error" showIcon   title={<>企业微信授权未完成。请检查应用配置、账号成员映射或重新授权。</>} />}
        {status.isPending && <Alert type="info" role="status"  title={<>正在检查企业微信配置…</>} />}
        {status.error && <Alert type="error" showIcon   title={<><p>无法读取企业微信配置：{messageOf(status.error)}</p><Button onClick={() => void status.refetch()}>重新检查配置</Button></>} />}
        {status.data && !status.data.oauth_available && <Alert className="wecom-notice" type="info" showIcon description={<><h2>{status.data.server_configured ? '当前环境仅支持预览' : '企业微信尚未配置'}</h2><p>{status.data.server_configured ? '实际授权与分享需要在已配置的 HTTPS 正式入口进行。' : '管理员配置企业微信应用并完成可信域名设置后，即可启用授权与分享。'}</p><a href="/admin?view=groups">前往后台管理客户群</a></>} />}
        {!isWecom && <Alert className="wecom-notice" type="info" showIcon description={<><strong>当前为普通浏览器</strong><p>可登录后台账号查看已授权学校的草稿。发送时请从企业微信客户群的应用入口打开此页。</p></>} />}
        {status.data?.oauth_available && !status.data.client_configured && <Alert className="wecom-notice" type="info" showIcon description={<><h2>企业微信登录可用，客户端分享尚未配置</h2><p>请联系管理员补齐或修正应用 AgentID；仍可登录并预览草稿。</p></>} />}
        {session.isPending ? <Alert type="info" role="status"  title={<>正在确认企业微信身份…</>} /> : !user ? <Alert className="wecom-notice" type="info" showIcon description={<><h2>登录后查看分享草稿</h2><p>请使用已获得学校运营授权的账号登录。</p><a href="/admin">登录运营后台</a>{status.data?.oauth_available && <p><a href={oauthUrl}>企业微信授权登录</a></p>}</>} /> : <p className="wecom-identity">当前账号：{user.display_name} · {user.wecom_authenticated ? '已通过企业微信身份认证' : '后台账号预览模式'} {!user.wecom_authenticated && status.data?.oauth_available && <a href={oauthUrl}>企业微信授权登录</a>}</p>}
        {sessionError && <Alert type="error" showIcon   title={<><p>会话查询失败：{messageOf(session.error)}</p><Button onClick={() => void session.refetch()}>重新查询身份</Button></>} />}
        {schools.error && <Alert type="error" showIcon   title={<><p>学校加载失败：{messageOf(schools.error)}</p><Button onClick={() => void schools.refetch()}>重新加载学校</Button></>} />}
        {user && schools.isPending && <Alert type="info" role="status"  title={<>正在加载学校…</>} />}
        {user && schools.isSuccess && !permittedSchools.length && <Empty className="empty-state" description={<><h2>暂无可运营的学校</h2><p>请联系管理员分配学校运营权限。</p></>} />}
        {!!permittedSchools.length && <><Field label={<>学校</>}><Select aria-label="学校" className="full-width" disabled={busy || checking} value={s} onChange={value => { setSchool(value); setDraftPage(1); setHistoryPage(1); setSelected(''); setChat(''); setResend(''); setReason(''); setRelatedPage(1); }}><Select.Option value="">请选择学校</Select.Option>{permittedSchools.map(v => <Select.Option key={v.id} value={v.id}>{v.name}</Select.Option>)}</Select></Field>
          {!s && <Alert type="error" showIcon   title={<>链接中的学校不在当前账号授权范围内，请重新选择。</>} />}
          {!!s && <><Field label={<>待分享草稿</>}><Select aria-label="待分享草稿" className="full-width" disabled={busy || checking} value={selected} onChange={value => { setSelected(value); setChat(''); setResend(''); setReason(''); setRelatedPage(1); }}><Select.Option value="">请选择</Select.Option>{draftOptions.map(d => <Select.Option key={d.id} value={d.id}>{d.group_name} · {d.content.slice(0, 35)}</Select.Option>)}</Select></Field>
          {drafts.isPending && <Alert type="info" role="status"  title={<>正在加载草稿…</>} />}
          {drafts.isSuccess && !drafts.data.items.length && <Empty className="empty-state" description={<><h2>暂无分享草稿</h2><p>请先在后台审核内容、发布模板，再生成草稿。</p><a href={`/admin?school=${s}&view=delivery-drafts`}>前往生成草稿</a></>} />}
          <PaginationControls page={draftPage} more={drafts.data?.page?.has_more} busy={busy || drafts.isFetching} change={setDraftPage} subject="草稿" />
          {selected && detail.isPending && <Alert type="info" role="status"  title={<>正在读取草稿…</>} />}
          {detail.error && <Alert type="error" showIcon   title={<>草稿读取失败：{messageOf(detail.error)}<Button onClick={() => void detail.refetch()}>重新读取草稿</Button></>} />}
          {draft && <Card className="draft-preview"><h2>{draft.group_name}</h2><pre className="text-content">{draft.content}</pre><small>模板版本：{draft.template_version_id}</small><p>目标客户群：{draft.group_name}（{draft.chat_id}）</p>
            <Button onClick={check} disabled={!canSend || busy || checking}>{checking ? '正在核对客户群…' : '读取并核对当前客户群'}</Button>{chat && <p>当前群：{chat}</p>}
            <p>请在企业微信中打开“{draft.group_name}”，从客户群聊天工具栏进入本应用，再选择此学校和草稿。直接打开预览链接不会进入目标群。</p>
            {related.isPending && <Alert type="info" role="status"  title={<>正在核对相关发送历史…</>} />}
            {related.error && <Alert type="error" showIcon   title={<>相关历史读取失败：{messageOf(related.error)}<Button onClick={() => void related.refetch()}>重新读取相关历史</Button></>} />}
            {related.data?.blocked && <Alert type="error" showIcon   title={<>相关内容仍有执行中或未核对的尝试，请先在后台分享记录完成人工核对。</>} />}
            {canResend && <Disclosure title={<>明确再次分享</>}><p>先核对历史结果，再选择上次尝试并填写原因。</p><Select className="full-width" aria-label="引用历史尝试" disabled={busy} value={resend} onChange={value => setResend(value)}><Select.Option value="">首次分享</Select.Option>{related.data?.items.map(h => <Select.Option key={h.id} value={h.id}>{String(h.group_name ?? '')} · {String(h.created_at ?? '')} · {stateNames[h.state] ?? h.state} · {h.id}</Select.Option>)}</Select><PaginationControls page={relatedPage} more={related.data?.page.has_more} busy={busy || related.isFetching} change={setRelatedPage} subject="相关历史" /><p>须引用的最近尝试：{related.data?.latest_attempt_id ?? '无'}</p><Input aria-label="再次分享原因" disabled={busy} value={reason} onChange={e => setReason(e.target.value)}/><Button disabled={busy || !resend || !reason.trim() || related.data?.blocked || resend !== related.data?.latest_attempt_id} onClick={() => { try { sessionStorage.removeItem(`wekeeper.attempt:${user!.id}:${s}:${draft.id}`); attempted.current.delete(draft.id); setMessage('已准备新的操作，请再次核对正文并点击分享。'); } catch (e) { setError(messageOf(e)); } }}>准备明确再次分享</Button></Disclosure>}
            <Button type="primary" block disabled={!canSend || busy || checking || chat !== draft.chat_id || !chat || attempted.current.has(draft.id) || !related.data || related.data.blocked || (!!related.data.latest_attempt_id && (!canResend || resend !== related.data.latest_attempt_id || !reason.trim()))} onClick={share}>{busy ? '等待客户端结果…' : '确认正文并在当前群分享'}</Button>
          </Card>}
          {drafts.error && <Alert type="error" showIcon   title={<><p>草稿加载失败：{messageOf(drafts.error)}</p><Button onClick={() => void drafts.refetch()}>重新加载草稿</Button></>} />}
          <h2>最近尝试</h2><p>客户端结果与人工核对独立保存。结果不确定时先查看目标群，不会自动重试。</p>
          {history.isPending && <Alert type="info" role="status"  title={<>正在加载分享记录…</>} />}{history.isSuccess && !history.data.items.length && <p>暂无分享记录</p>}
          {history.error && <Alert type="error" showIcon   title={<><p>分享记录加载失败：{messageOf(history.error)}</p><Button onClick={() => void history.refetch()}>重新加载记录</Button></>} />}
          {history.data?.items.map(h => <Card className="wecom-attempt" key={h.id}><strong>{stateNames[h.state] ?? h.state}</strong><p>{String(h.group_name ?? '')} · {String(h.actor_name ?? h.actor_id)} · {String(h.created_at ?? '')}</p><pre className="text-content">{String(h.content ?? '')}</pre><small>{h.id}</small>{h.reconciliation && <p>核对结论：{stateNames[h.reconciliation] ?? h.reconciliation}</p>}</Card>)}
          <PaginationControls page={historyPage} more={history.data?.page?.has_more} busy={history.isFetching} change={setHistoryPage} subject="分享记录" />
          </>}
        </>}
        {error && <Alert type="error" showIcon   title={<>{error}</>} />}{message && <Alert type="info" role="status"  title={<>{message}</>} />}
    </section>;
}

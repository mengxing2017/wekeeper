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
    const drafts = useQuery({ queryKey: ['business', s, 'delivery-drafts'], queryFn: () => workspace.list<Draft>(s, 'delivery-drafts'), enabled: !!s, retry: false });
    const history = useQuery({ queryKey: ['business', s, 'delivery-attempts'], queryFn: () => workspace.list<Attempt>(s, 'delivery-attempts'), enabled: !!s, retry: false, refetchInterval: 30000 });
    const draft = drafts.data?.find(d => d.id === selected);
    const [chat, setChat] = useState(''), [error, setError] = useState(''), [message, setMessage] = useState(''), [busy, setBusy] = useState(false), [checking, setChecking] = useState(false), [resend, setResend] = useState(''), [reason, setReason] = useState('');
    const lock = useRef(false);
    const attempted = useRef(new Set<string>());
    const canSend = !!user?.wecom_authenticated && isWecom && !!status.data?.oauth_available;
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
        finally { setBusy(false); lock.current = false; void history.refetch(); }
    }
    const sessionError = session.error && !('status' in session.error && session.error.status === 401);
    return <section className="wecom-workspace">
        <div className="wecom-title"><span className="workspace-badge">{canSend ? '企业成员工作空间' : '预览与接入'}</span><h1>企业微信人工分享</h1><p>选择学校与草稿，核对当前客户群，再人工确认分享。</p></div>
        <ol className="wecom-steps"><li>1 · 确认身份</li><li>2 · 预览正文</li><li>3 · 核对并分享</li></ol>
        {params.has('oauth_error') && <p role="alert">企业微信授权未完成。请检查应用配置、账号成员映射或重新授权。</p>}
        {status.isPending && <p role="status">正在检查企业微信配置…</p>}
        {status.error && <div role="alert"><p>无法读取企业微信配置：{messageOf(status.error)}</p><button onClick={() => void status.refetch()}>重新检查配置</button></div>}
        {status.data && !status.data.oauth_available && <div className="wecom-notice"><h2>{status.data.configured ? '当前环境仅支持预览' : '企业微信尚未配置'}</h2><p>{status.data.configured ? '实际授权与分享需要在已配置的 HTTPS 正式入口进行。' : '管理员配置企业微信应用并完成可信域名设置后，即可启用授权与分享。'}</p><a href="/admin?view=groups">前往后台管理客户群</a></div>}
        {!isWecom && <div className="wecom-notice"><strong>当前为普通浏览器</strong><p>可登录后台账号查看已授权学校的草稿。发送时请从企业微信客户群的应用入口打开此页。</p></div>}
        {session.isPending ? <p role="status">正在确认企业微信身份…</p> : !user ? <div className="wecom-notice"><h2>登录后查看分享草稿</h2><p>请使用已获得学校运营授权的账号登录。</p><a href="/admin">登录运营后台</a>{status.data?.oauth_available && <p><a href={oauthUrl}>企业微信授权登录</a></p>}</div> : <p className="wecom-identity">当前账号：{user.display_name} · {user.wecom_authenticated ? '已通过企业微信身份认证' : '后台账号预览模式'} {!user.wecom_authenticated && status.data?.oauth_available && <a href={oauthUrl}>企业微信授权登录</a>}</p>}
        {sessionError && <div role="alert"><p>会话查询失败：{messageOf(session.error)}</p><button onClick={() => void session.refetch()}>重新查询身份</button></div>}
        {schools.error && <div role="alert"><p>学校加载失败：{messageOf(schools.error)}</p><button onClick={() => void schools.refetch()}>重新加载学校</button></div>}
        {user && schools.isPending && <p role="status">正在加载学校…</p>}
        {user && schools.isSuccess && !permittedSchools.length && <div className="empty-state"><h2>暂无可运营的学校</h2><p>请联系管理员分配学校运营权限。</p></div>}
        {!!permittedSchools.length && <><label>学校<select disabled={busy || checking} value={s} onChange={e => { setSchool(e.target.value); setSelected(''); setChat(''); setResend(''); setReason(''); }}><option value="">请选择学校</option>{permittedSchools.map(v => <option key={v.id} value={v.id}>{v.name}</option>)}</select></label>
          {!s && <p role="alert">链接中的学校不在当前账号授权范围内，请重新选择。</p>}
          {!!s && <><label>待分享草稿<select disabled={busy || checking} value={selected} onChange={e => { setSelected(e.target.value); setChat(''); setResend(''); setReason(''); }}><option value="">请选择</option>{drafts.data?.map(d => <option key={d.id} value={d.id}>{d.group_name} · {d.content.slice(0, 35)}</option>)}</select></label>
          {drafts.isPending && <p role="status">正在加载草稿…</p>}
          {drafts.isSuccess && !drafts.data.length && <div className="empty-state"><h2>暂无分享草稿</h2><p>请先在后台审核内容、发布模板，再生成草稿。</p><a href={`/admin?school=${s}&view=delivery-drafts`}>前往生成草稿</a></div>}
          {selected && drafts.isSuccess && !draft && <p role="alert">草稿不存在或无权查看，请重新选择。</p>}
          {draft && <article><h2>{draft.group_name}</h2><pre className="text-content">{draft.content}</pre><small>模板版本：{draft.template_version_id}</small><p>目标客户群：{draft.chat_id}</p>
            <button onClick={check} disabled={!canSend || busy || checking}>{checking ? '正在核对客户群…' : '读取并核对当前客户群'}</button>{chat && <p>当前群：{chat}</p>}
            {canResend && <details><summary>明确再次分享</summary><p>先核对历史结果，再选择上次尝试并填写原因。</p><select aria-label="引用历史尝试" disabled={busy} value={resend} onChange={e => setResend(e.target.value)}><option value="">首次分享</option>{history.data?.filter(h => h.draft_id === draft.id).map(h => <option key={h.id} value={h.id}>{h.id} · {stateNames[h.state] ?? h.state}</option>)}</select><input aria-label="再次分享原因" disabled={busy} value={reason} onChange={e => setReason(e.target.value)}/><button disabled={busy || !resend || !reason.trim()} onClick={() => { try { sessionStorage.removeItem(`wekeeper.attempt:${user!.id}:${s}:${draft.id}`); attempted.current.delete(draft.id); setMessage('已准备新的操作，请再次核对正文并点击分享。'); } catch (e) { setError(messageOf(e)); } }}>准备明确再次分享</button></details>}
            <button disabled={!canSend || busy || checking || chat !== draft.chat_id || !chat || attempted.current.has(draft.id)} onClick={share}>{busy ? '等待客户端结果…' : '确认正文并在当前群分享'}</button>
          </article>}
          {drafts.error && <div role="alert"><p>草稿加载失败：{messageOf(drafts.error)}</p><button onClick={() => void drafts.refetch()}>重新加载草稿</button></div>}
          <h2>最近尝试</h2><p>客户端结果与人工核对独立保存。结果不确定时先查看目标群，不会自动重试。</p>
          {history.isPending && <p role="status">正在加载分享记录…</p>}{history.isSuccess && !history.data.length && <p>暂无分享记录</p>}
          {history.error && <div role="alert"><p>分享记录加载失败：{messageOf(history.error)}</p><button onClick={() => void history.refetch()}>重新加载记录</button></div>}
          {history.data?.map(h => <div className="wecom-attempt" key={h.id}><strong>{stateNames[h.state] ?? h.state}</strong><small>{h.id}</small>{h.reconciliation && <p>核对结论：{stateNames[h.reconciliation] ?? h.reconciliation}</p>}</div>)}
          </>}
        </>}
        {error && <p role="alert">{error}</p>}{message && <p role="status">{message}</p>}
    </section>;
}

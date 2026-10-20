import { useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { identity, workspace, wecomOAuthUrl, type Draft, type Attempt, type ClientState } from './api';
import { currentChat, sendText, sendOutcome } from './sdk';
export function ShareWorkspace() {
    const session = useQuery({ queryKey: ['session'], queryFn: identity.session, retry: false });
    const params = new URLSearchParams(location.search);
    const [school, setSchool] = useState(params.get('school') ?? '');
    const [selected, setSelected] = useState(params.get('draft') ?? '');
    const schools = useQuery({ queryKey: ['schools'], queryFn: workspace.schools, enabled: !!session.data });
    const s = school || schools.data?.[0]?.id || '';
    const drafts = useQuery({ queryKey: ['business', s, 'delivery-drafts'], queryFn: () => workspace.list<Draft>(s, 'delivery-drafts'), enabled: !!s && !!session.data });
    const history = useQuery({ queryKey: ['business', s, 'delivery-attempts'], queryFn: () => workspace.list<Attempt>(s, 'delivery-attempts'), enabled: !!s && !!session.data, refetchInterval: 30000 });
    const draft = drafts.data?.find(d => d.id === selected);
    const [chat, setChat] = useState(''), [error, setError] = useState(''), [message, setMessage] = useState(''), [busy, setBusy] = useState(false), [resend, setResend] = useState(''), [reason, setReason] = useState('');
    const lock = useRef(false);
    const attempted = useRef(new Set<string>());
    async function check() { try {
        setChat(await currentChat());
        setError('');
    }
    catch (e) {
        setError((e as Error).message);
    } }
    async function share() {
        if (!draft || lock.current || attempted.current.has(draft.id))
            return;
        lock.current = true;
        setBusy(true);
        setError('');
        setMessage('');
        let attempt: Attempt | undefined;
        try {
            const actual = await currentChat();
            if (actual !== chat || actual !== draft.chat_id)
                throw new Error('当前群与草稿目标不一致，请重新核对');
            // Store the key BEFORE requesting preparation. A lost response never causes a new key on reload.
            const storageKey = `wekeeper.attempt:${s}:${draft.id}`;
            let key = sessionStorage.getItem(storageKey);
            if (!key) {
                key = crypto.randomUUID();
                sessionStorage.setItem(storageKey, key);
            }
            attempt = await workspace.prepare(s, draft.id, key, { current_chat_id: actual, resend_of: resend || undefined, reason });
            attempted.current.add(draft.id);
            if (attempt.state !== 'prepared')
                throw new Error('该操作已有结果，请先在分享记录核对');
            const invokedKey = `wekeeper.invoked:${attempt.id}`;
            if (localStorage.getItem(invokedKey))
                throw new Error('此尝试已在客户端执行过，请人工核对，不能重复调用发送');
            localStorage.setItem(invokedKey, 'true');
            await workspace.claim(s, attempt.id);
            const state: ClientState = await sendOutcome(() => sendText(draft.content));
            await workspace.result(s, attempt.id, state);
            setMessage(state === 'client_reported_success' ? '客户端报告调用成功，请在目标群核对实际消息。' : '此次操作未确认成功，请查看分享记录并人工核对。');
        }
        catch (e) {
            setError(`${(e as Error).message}${attempt ? '；本次不会自动重发。' : ''}`);
        }
        finally {
            setBusy(false);
            lock.current = false;
            void history.refetch();
        }
    }
    if (session.isPending)
        return <p role="status">正在确认企业微信身份…</p>;
    if (!session.data?.user.wecom_authenticated)
        return <section><h1>企业微信人工分享</h1><p>请使用已获得学校运营授权的企业成员登录。</p><a href={wecomOAuthUrl({school: params.get('school') ?? undefined, draft: params.get('draft') ?? undefined})}>企业微信授权登录</a></section>;
    return <section><h1>预览并人工分享</h1><p>仅发送到当前客户群。结果不确定时先人工核对，不会自动重试。</p><label>学校<select value={s} onChange={e => { setSchool(e.target.value); setSelected(''); setChat(''); }}>{schools.data?.map(v => <option key={v.id} value={v.id}>{v.name}</option>)}</select></label><button onClick={check} disabled={busy}>读取并核对当前客户群</button>{chat && <p>当前群：{chat}</p>}<label>待分享草稿<select value={selected} onChange={e => setSelected(e.target.value)}><option value="">请选择</option>{drafts.data?.map(d => <option key={d.id} value={d.id}>{d.group_name} · {d.content.slice(0, 35)}</option>)}</select></label>
 {draft && <article><h2>{draft.group_name}</h2><pre className="text-content">{draft.content}</pre><small>模板版本：{draft.template_version_id}</small><details><summary>明确再次分享</summary><p>先在后台核对历史结果，再选择上次尝试并填写原因。</p><select aria-label="引用历史尝试" value={resend} onChange={e => setResend(e.target.value)}><option value="">首次分享</option>{history.data?.map(h => <option key={h.id} value={h.id}>{h.id} · {h.state}</option>)}</select><input aria-label="再次分享原因" value={reason} onChange={e => setReason(e.target.value)}/><button disabled={busy} onClick={() => { if (resend && reason.trim()) {
        sessionStorage.removeItem(`wekeeper.attempt:${s}:${draft.id}`);
        attempted.current.delete(draft.id);
        setMessage('已准备新的操作，请再次核对正文并点击分享。');
    } }}>准备明确再次分享</button></details><button disabled={busy || chat !== draft.chat_id || !chat || attempted.current.has(draft.id)} onClick={share}>{busy ? '等待客户端结果…' : '确认正文并在当前群分享'}</button></article>}
 {(error || drafts.error || history.error) && <p role="alert">{error || drafts.error?.message || history.error?.message}</p>}{message && <p role="status">{message}</p>}<h2>最近尝试</h2>{history.data?.map(h => <p key={h.id}>{h.state} · {h.id} {h.reconciliation && `· ${h.reconciliation}`}</p>)}
 </section>;
}

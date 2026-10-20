import { useState, type FormEvent, type CSSProperties } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { SessionGate, label } from '../admin/workspace';
import { identity, workspace, type User, type Post } from './api';
export default function Community() { return <main className="community-shell"><header><Link to="/community">WeKeeper 社区</Link> · <Link to="/admin">运营后台</Link></header><SessionGate>{user => <Content user={user}/>}</SessionGate></main>; }
function Content({ user }: {
    user: User;
}) {
    const params = useParams();
    const cache = useQueryClient();
    const schools = useQuery({ queryKey: ['schools'], queryFn: workspace.schools });
    const directory = useQuery({ queryKey: ['directory'], queryFn: workspace.directory });
    const [selected, setSelected] = useState('');
    const school = schools.data?.find(v => v.id === (params.school || selected || user.bound_school_id || schools.data[0]?.id));
    const s = school?.id ?? '';
    const [search, setSearch] = useState(''), [view, setView] = useState('feed'), [page, setPage] = useState(1), [error, setError] = useState(''), [busy, setBusy] = useState(false), [images, setImages] = useState<string[]>([]), [editing, setEditing] = useState<Post>();
    const posts = useQuery({ queryKey: ['business', s, 'feed', search, view, page], queryFn: () => workspace.list<Post>(s, 'posts', `?${new URLSearchParams({ search, page: String(page), page_size: '20', mine: String(view === 'mine'), ...(['collect', 'following'].includes(view) ? { kind: view } : {}) })}`), enabled: !!s });
    const detail = useQuery({ queryKey: ['business', s, 'post', params.id], queryFn: () => workspace.getPost(s, params.id!), enabled: !!s && !!params.id });
    const comments = useQuery({ queryKey: ['business', s, 'post-comments', params.id], queryFn: () => workspace.comments(s, params.id!), enabled: !!s && !!params.id });
    const reactions = useQuery({ queryKey: ['business', s, 'reactions'], queryFn: () => workspace.list(s, 'reactions'), enabled: !!s });
    const resources = useQuery({ queryKey: ['business', s, 'resources'], queryFn: () => workspace.list(s, 'resources'), enabled: !!s });
    const notices = useQuery({ queryKey: ['business', s, 'notifications'], queryFn: () => workspace.list(s, 'notifications'), enabled: !!s });
    const follows = useQuery({ queryKey: ['business', s, 'follows'], queryFn: () => workspace.list(s, 'follows'), enabled: !!s });
    async function run(action: () => Promise<unknown>) { setBusy(true); setError(''); try {
        await action();
        await cache.invalidateQueries({ queryKey: ['business'] });
    }
    catch (e) {
        setError((e as Error).message);
    }
    finally {
        setBusy(false);
    } }
    async function publish(e: FormEvent<HTMLFormElement>) { e.preventDefault(); const f = new FormData(e.currentTarget); await run(async () => { await workspace.mutate(s, editing ? `posts/${editing.id}` : 'posts', { title: f.get('title'), content: f.get('content'), images, tags: String(f.get('tags') ?? '').split(',').map(v => v.trim()).filter(Boolean), revision: editing?.revision }, editing ? 'PUT' : 'POST'); setEditing(undefined); setImages([]); setView('mine'); }); }
    if (!s)
        return <section><h1>绑定学校</h1><p>绑定后不能自行换校，请确认学校。</p>{directory.data?.map(v => <button key={v.id} disabled={busy} onClick={() => run(async () => { await workspace.bind(v.id); await cache.invalidateQueries({ queryKey: ['schools'] }); await cache.invalidateQueries({ queryKey: ['session'] }); })}>{v.name}</button>)}{error && <p role="alert">{error}</p>}</section>;
    const list = params.id ? (detail.data ? [detail.data] : []) : posts.data ?? [];
    const filtered = list.filter(p => view !== 'collect' || reactions.data?.some(r => r.post_id === p.id && r.kind === 'collect')).filter(p => view !== 'following' || follows.data?.some(f => f.id === p.author_id));
    return <div style={{ '--school-color': label(school?.config.theme_color) || '#25624b' } as CSSProperties}><h1>{school?.name}</h1><select aria-label="学校" value={s} onChange={async (e) => { await cache.cancelQueries({ queryKey: ['business'] }); cache.removeQueries({ queryKey: ['business'] }); setSelected(e.target.value); }}>{schools.data?.map(v => <option key={v.id} value={v.id}>{v.name}</option>)}</select>
 <nav className="community-nav">{[['feed', '发现'], ['mine', '我的发布'], ['collect', '收藏'], ['following', '关注动态'], ['publish', '发布'], ['notifications', '消息'], ['resources', '校园服务'], ['profile', '个人信息'], ['feedback', '反馈']].map(([key, title]) => <button key={key} onClick={() => { setView(key); setPage(1); }}>{title}</button>)}</nav>
 {(error || posts.error || detail.error) && <p role="alert">{error || posts.error?.message || detail.error?.message}</p>}
 {['feed', 'mine', 'collect', 'following'].includes(view) && <><form onSubmit={e => { e.preventDefault(); setSearch(String(new FormData(e.currentTarget).get('search'))); setPage(1); }}><input name="search" aria-label="搜索校园内容"/><button>搜索</button></form>{filtered.map(p => <article className="community-card" key={p.id}><h2><Link to={`/community/${s}/posts/${p.id}`}>{p.title}</Link></h2><p className="text-content">{p.content}</p>{p.images.map(src => <img key={src} src={src} alt="帖子图片" loading="lazy"/>)}<p>{label(p.author_name)} · {p.audit_status} · {p.visibility}</p><div className="community-nav">{(['like', 'collect'] as const).map(kind => { const active = reactions.data?.some(r => r.post_id === p.id && r.kind === kind); return <button key={kind} disabled={busy || p.audit_status !== 'approved' || p.visibility !== 'visible'} onClick={() => run(() => workspace.mutate(s, `posts/${p.id}/reactions/${kind}`, undefined, active ? 'DELETE' : 'PUT'))}>{active ? '取消' : ''}{kind === 'like' ? '点赞' : '收藏'}</button>; })}
 {p.author_id !== user.id && <button disabled={busy} onClick={() => run(() => workspace.mutate(s, `follows/${p.author_id}`, undefined, follows.data?.some(f => f.id === p.author_id) ? 'DELETE' : 'PUT'))}>关注 / 取消关注作者</button>}<button disabled={busy} onClick={() => { const content = window.prompt('举报原因'); if (content)
                void run(() => workspace.mutate(s, 'cases', { kind: 'report', post_id: p.id, content })); }}>举报</button>
 {p.author_id === user.id && <><button onClick={() => { setEditing(p); setImages(p.images); setView('publish'); }}>编辑</button><button disabled={busy} onClick={() => { if (window.confirm('确认删除这条内容？'))
                void run(() => workspace.mutate(s, `posts/${p.id}/visibility`, { revision: p.revision, visibility: 'deleted', top_sort: 0 }, 'PUT')); }}>删除</button></>}</div></article>)}{!filtered.length && <p>暂无内容</p>}{!params.id && <div><button disabled={page === 1} onClick={() => setPage(page - 1)}>上一页</button> {page} <button disabled={(posts.data?.length ?? 0) < 20} onClick={() => setPage(page + 1)}>下一页</button></div>}
 {params.id && <section><h2>评论</h2><form onSubmit={e => { e.preventDefault(); const form = e.currentTarget; const content = String(new FormData(form).get('content')); void run(async () => { await workspace.mutate(s, `posts/${params.id}/comments`, { content }); form.reset(); }); }}><textarea name="content" required maxLength={2000}/><button disabled={busy}>提交评论</button></form>{comments.data?.map(c => <article key={c.id}><p>{label(c.author_name)}：{label(c.content)}</p><button disabled={busy} onClick={() => { const content = window.prompt('回复内容'); if (content)
            void run(() => workspace.mutate(s, `posts/${params.id}/comments`, { content, parent_id: c.id })); }}>回复</button>{c.author_id === user.id && <button disabled={busy} onClick={() => run(() => workspace.mutate(s, `comments/${c.id}`, { revision: c.revision }, 'DELETE'))}>删除</button>}</article>)}</section>}</>}
 {view === 'publish' && <form className="business-form" onSubmit={publish} key={editing?.id ?? 'new'}><h2>{editing ? '编辑内容' : '发布内容'}</h2><label>标题<input name="title" required maxLength={200} defaultValue={editing?.title}/></label><label>正文<textarea name="content" required maxLength={20000} defaultValue={editing?.content}/></label><label>标签（逗号分隔）<input name="tags" defaultValue={editing?.tags.join(',')}/></label><label>图片（最多9张）<input type="file" accept="image/png,image/jpeg,image/webp,image/gif" multiple disabled={busy} onChange={e => { const files = Array.from(e.target.files ?? []); void run(async () => { if (images.length + files.length > 9)
        throw new Error('最多9张图片'); const uploaded = []; for (const f of files)
        uploaded.push((await workspace.upload(s, f)).url); setImages([...images, ...uploaded]); }); }}/></label>{images.map(src => <div key={src}><img src={src} alt="上传预览" width={100}/><button type="button" onClick={() => setImages(images.filter(v => v !== src))}>移除</button></div>)}<button disabled={busy}>提交审核</button></form>}
 {view === 'notifications' && <>{notices.data?.map(n => <p key={n.id}>{label(n.kind)} · {label(n.created_at)} <button disabled={busy || !!n.read_at} onClick={() => run(() => workspace.mutate(s, `notifications/${n.id}/read`, undefined, 'PUT'))}>{n.read_at ? '已读' : '标记已读'}</button></p>)}</>}
 {view === 'resources' && resources.data?.filter(r => r.kind !== 'blocked_word').map(r => <article className="community-card" key={r.id}><h2>{label(r.title)}</h2><p className="text-content">{label(r.content)}</p>{r.url && <a href={label(r.url)} rel="noreferrer" target="_blank" onClick={() => { if (r.kind === 'banner')
        void run(() => workspace.mutate(s, `resources/${r.id}/click`)); }}>打开 {label(r.title)}</a>}</article>)}
 {view === 'profile' && <form className="business-form" onSubmit={e => { e.preventDefault(); const name = String(new FormData(e.currentTarget).get('name')); void run(async () => { await identity.profile(name); await cache.invalidateQueries({ queryKey: ['session'] }); }); }}><label>昵称<input name="name" defaultValue={user.display_name} required/></label><button disabled={busy}>更新昵称</button><h2>我的关注</h2>{follows.data?.map(f => <p key={f.id}>{label(f.display_name)} <button type="button" onClick={() => run(() => workspace.mutate(s, `follows/${f.id}`, undefined, 'DELETE'))}>取消关注</button></p>)}</form>}
 {view === 'feedback' && <form className="business-form" onSubmit={e => { e.preventDefault(); const f = new FormData(e.currentTarget); void run(() => workspace.mutate(s, 'cases', { kind: 'feedback', content: f.get('content'), contact: f.get('contact') })); }}><label>意见反馈<textarea name="content" required maxLength={500}/></label><label>联系方式<input name="contact"/></label><button disabled={busy}>提交反馈</button></form>}
 </div>;
}

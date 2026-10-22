import { formatDate } from '../../shared/format-date'
import { PaginationControls } from '../../shared/pagination-controls'
import { Field } from '../../shared/field'
import { useDialogs } from '../../shared/use-dialogs'
import { Alert, Button, Form, Input, Select, Upload, ConfigProvider , Card  } from 'antd'
import { useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { SessionGate, label } from '../admin/workspace';
import { identity, workspace, type User, type Post } from './api';
export default function Community() { return <main className="community-shell"><header><Link to="/community">WeKeeper 社区</Link> · <Link to="/admin">运营后台</Link></header><SessionGate>{user => <Content user={user}/>}</SessionGate></main>; }
function Content({ user }: {
    user: User;
}) {
    const dialogs = useDialogs();
    const [commentForm] = Form.useForm();
    const params = useParams();
    const cache = useQueryClient();
    const schools = useQuery({ queryKey: ['schools'], queryFn: workspace.schools });
    const directory = useQuery({ queryKey: ['directory'], queryFn: workspace.directory });
    const [selected, setSelected] = useState('');
    const school = schools.data?.find(v => v.id === (params.school || selected || user.bound_school_id || schools.data[0]?.id));
    const s = school?.id ?? '';
    const [search, setSearch] = useState(''), [view, setView] = useState('feed'), [page, setPage] = useState(1), [error, setError] = useState(''), [busy, setBusy] = useState(false), [images, setImages] = useState<string[]>([]), [editing, setEditing] = useState<Post>();
    const posts = useQuery({ queryKey: ['business', s, 'feed', search, view, page], queryFn: () => workspace.list<Post>(s, 'posts', `?${new URLSearchParams({ search, 'page[number]': String(page), 'page[size]': '20', mine: String(view === 'mine'), ...(['collect', 'following'].includes(view) ? { kind: view } : {}) })}`), enabled: !!s });
    const detail = useQuery({ queryKey: ['business', s, 'post', params.id], queryFn: () => workspace.getPost(s, params.id!), enabled: !!s && !!params.id });
    const [commentPage, setCommentPage] = useState(1), [noticePage, setNoticePage] = useState(1);
    const comments = useQuery({ queryKey: ['business', s, 'post-comments', params.id, commentPage], queryFn: () => workspace.comments(s, params.id!, commentPage), enabled: !!s && !!params.id });
    const reactions = useQuery({ queryKey: ['business', s, 'reactions'], queryFn: () => workspace.list(s, 'reactions'), enabled: !!s });
    const resources = useQuery({ queryKey: ['business', s, 'resources'], queryFn: () => workspace.list(s, 'resources'), enabled: !!s });
    const notices = useQuery({ queryKey: ['business', s, 'notifications', noticePage], queryFn: () => workspace.list(s, 'notifications', `?page[number]=${noticePage}&page[size]=30`), enabled: !!s });
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
    async function publish(values: { title: string; content: string; tags?: string }) { await run(async () => { await workspace.mutate(s, editing ? `posts/${editing.id}` : 'posts', { title: values.title, content: values.content, images, tags: String(values.tags ?? '').split(',').map(v => v.trim()).filter(Boolean), revision: editing?.revision }, editing ? 'PUT' : 'POST'); setEditing(undefined); setImages([]); setView('mine'); }); }
    if (!s)
        return <section><h1>绑定学校</h1><p>绑定后不能自行换校，请确认学校。</p>{directory.data?.map(v => <Button key={v.id} disabled={busy} onClick={() => run(async () => { await workspace.bind(v.id); await cache.invalidateQueries({ queryKey: ['schools'] }); await cache.invalidateQueries({ queryKey: ['session'] }); })}>{v.name}</Button>)}{error && <Alert type="error" showIcon   title={<>{error}</>} />}</section>;
    const list = params.id ? (detail.data ? [detail.data] : []) : posts.data?.items ?? [];
    const filtered = list.filter(p => view !== 'collect' || reactions.data?.items.some(r => r.post_id === p.id && r.kind === 'collect')).filter(p => view !== 'following' || follows.data?.items.some(f => f.id === p.author_id));
    return <ConfigProvider theme={{ token: { colorPrimary: label(school?.config.theme_color) || '#08786c' } }}><div>{dialogs.contextHolder}<h1>{school?.name}</h1><Select className="full-width" aria-label="学校" value={s} onChange={async (value) => { await cache.cancelQueries({ queryKey: ['business'] }); cache.removeQueries({ queryKey: ['business'] }); setSelected(value); }}>{schools.data?.map(v => <Select.Option key={v.id} value={v.id}>{v.name}</Select.Option>)}</Select>
 <nav className="community-nav">{[['feed', '发现'], ['mine', '我的发布'], ['collect', '收藏'], ['following', '关注动态'], ['publish', '发布'], ['notifications', '消息'], ['resources', '校园服务'], ['profile', '个人信息'], ['feedback', '反馈']].map(([key, title]) => <Button type={view === key ? 'primary' : 'default'} key={key} aria-current={view === key ? 'page' : undefined} onClick={() => { setView(key); setPage(1); }}>{title}</Button>)}</nav>
 {(error || posts.error || detail.error) && <Alert type="error" showIcon   title={<>{error || posts.error?.message || detail.error?.message}</>} />}
 {['feed', 'mine', 'collect', 'following'].includes(view) && <><Input.Search aria-label="搜索校园内容" enterButton="搜索" onSearch={value => { setSearch(value); setPage(1); }} />{filtered.map(p => <Card className="community-card" key={p.id}><h2><Link to={`/community/${s}/posts/${p.id}`}>{p.title}</Link></h2><p className="text-content">{p.content}</p>{p.images.map(src => <img key={src} src={src} alt="帖子图片" loading="lazy"/>)}<p>{label(p.author_name)} · {p.audit_status} · {p.visibility}</p><div className="community-nav">{(['like', 'collect'] as const).map(kind => { const active = reactions.data?.items.some(r => r.post_id === p.id && r.kind === kind); return <Button key={kind} disabled={busy || p.audit_status !== 'approved' || p.visibility !== 'visible'} onClick={() => run(() => workspace.mutate(s, `posts/${p.id}/reactions/${kind}`, undefined, active ? 'DELETE' : 'PUT'))}>{active ? '取消' : ''}{kind === 'like' ? '点赞' : '收藏'}</Button>; })}
 {p.author_id !== user.id && <Button disabled={busy} onClick={() => run(() => workspace.mutate(s, `follows/${p.author_id}`, undefined, follows.data?.items.some(f => f.id === p.author_id) ? 'DELETE' : 'PUT'))}>关注 / 取消关注作者</Button>}<Button disabled={busy} onClick={async () => { const content = await dialogs.prompt('举报原因'); if (content)
                void run(() => workspace.mutate(s, 'cases', { kind: 'report', post_id: p.id, content })); }}>举报</Button>
 {p.author_id === user.id && <><Button onClick={() => { setEditing(p); setImages(p.images); setView('publish'); }}>编辑</Button><Button disabled={busy} onClick={async () => { if (await dialogs.confirm('确认删除这条内容？'))
                void run(() => workspace.mutate(s, `posts/${p.id}/visibility`, { revision: p.revision, visibility: 'deleted', top_sort: 0 }, 'PUT')); }}>删除</Button></>}</div></Card>)}{!filtered.length && <p>暂无内容</p>}{!params.id && <PaginationControls page={page} more={posts.data?.page?.has_more} busy={posts.isFetching} change={setPage} />}
 {params.id && <section><h2>评论</h2><Form form={commentForm} layout="vertical" className="business-form" disabled={busy} onFinish={({ content }: { content: string }) => run(async () => {
      await workspace.mutate(s, `posts/${params.id}/comments`, { content }); commentForm.resetFields();
    })}><Field name="content" required label="评论内容"><Input.TextArea maxLength={2000} /></Field><Button type="primary" htmlType="submit" loading={busy}>提交评论</Button></Form>{comments.data?.items.map(c => <article key={c.id}><p>{label(c.author_name)}：{label(c.content)}</p><Button disabled={busy} onClick={async () => { const content = await dialogs.prompt('回复内容'); if (content)
            void run(() => workspace.mutate(s, `posts/${params.id}/comments`, { content, parent_id: c.id })); }}>回复</Button>{c.author_id === user.id && <Button disabled={busy} onClick={() => run(() => workspace.mutate(s, `comments/${c.id}`, { revision: c.revision }, 'DELETE'))}>删除</Button>}</article>)}<PaginationControls page={commentPage} more={comments.data?.page?.has_more} busy={comments.isFetching} change={setCommentPage} subject="评论" /></section>}</>}
 {view === 'publish' && <Form layout="vertical" className="business-form" onFinish={publish} disabled={busy} key={editing?.id ?? 'new'} initialValues={{ title: editing?.title, content: editing?.content, tags: editing?.tags.join(',') }}><h2>{editing ? '编辑内容' : '发布内容'}</h2><Field name="title" required label={<>标题</>}><Input name="title" maxLength={200}/></Field><Field name="content" required label={<>正文</>}><Input.TextArea name="content" maxLength={20000}/></Field><Field name="tags" label={<>标签（逗号分隔）</>}><Input name="tags"/></Field><div><p>图片（最多9张）</p><Upload accept="image/png,image/jpeg,image/webp,image/gif" multiple disabled={busy} showUploadList={false} beforeUpload={(_, files) => {
      if (_ === files[0]) void run(async () => {
        if (images.length + files.length > 9) throw new Error('最多9张图片');
        const uploaded = [...images];
        for (const file of files) {
          uploaded.push((await workspace.upload(s, file)).url);
          setImages([...uploaded]);
        }
      });
      return false;
    }}><Button disabled={busy} loading={busy}>选择图片</Button></Upload></div>{images.map(src => <div key={src}><img src={src} alt="上传预览" width={100}/><Button htmlType="button" onClick={() => setImages(images.filter(v => v !== src))}>移除</Button></div>)}<Button type="primary" htmlType="submit" loading={busy} disabled={busy}>提交审核</Button></Form>}
 {view === 'notifications' && <>{notices.data?.items.map(n => <p key={n.id}>{label(n.kind)} · {formatDate(n.created_at)} <Button disabled={busy || !!n.read_at} onClick={() => run(() => workspace.mutate(s, `notifications/${n.id}/read`, undefined, 'PUT'))}>{n.read_at ? '已读' : '标记已读'}</Button></p>)}<PaginationControls page={noticePage} more={notices.data?.page?.has_more} busy={notices.isFetching} change={setNoticePage} subject="消息" /></>}
 {view === 'resources' && resources.data?.items.filter(r => r.kind !== 'blocked_word').map(r => <Card className="community-card" key={r.id}><h2>{label(r.title)}</h2><p className="text-content">{label(r.content)}</p>{r.url && <a href={label(r.url)} rel="noreferrer" target="_blank" onClick={() => { if (r.kind === 'banner')
        void run(() => workspace.mutate(s, `resources/${r.id}/click`)); }}>打开 {label(r.title)}</a>}</Card>)}
 {view === 'profile' && <Form layout="vertical" className="business-form" initialValues={{ name: user.display_name }} disabled={busy} onFinish={({ name }: { name: string }) => run(async () => { await identity.profile(name); await cache.invalidateQueries({ queryKey: ['session'] }); })}><Field name="name" required label="昵称"><Input /></Field><Button type="primary" htmlType="submit" loading={busy} disabled={busy}>更新昵称</Button><h2>我的关注</h2>{follows.data?.items.map(f => <p key={f.id}>{label(f.display_name)} <Button htmlType="button" onClick={() => run(() => workspace.mutate(s, `follows/${f.id}`, undefined, 'DELETE'))}>取消关注</Button></p>)}</Form>}
 {view === 'feedback' && <Form layout="vertical" className="business-form" disabled={busy} onFinish={({ content, contact }: { content: string; contact?: string }) => run(() => workspace.mutate(s, 'cases', { kind: 'feedback', content, contact: contact ?? '' }))}><Field name="content" required label="意见反馈"><Input.TextArea maxLength={500} /></Field><Field name="contact" label="联系方式"><Input /></Field><Button type="primary" htmlType="submit" loading={busy} disabled={busy}>提交反馈</Button></Form>}
 </div></ConfigProvider>;
}

import { requestResource, requestCollection, requestDocument, decodeData, setCsrf, type PageMeta } from '../../http';
export { ApiError } from '../../http';
export type Json = string | number | boolean | null | Json[] | {
    [key: string]: Json;
};
export interface Row {
    id: string;
    [key: string]: Json;
}
export interface Grant {
    school_id: string;
    role: 'member' | 'operator' | 'admin';
}
export interface User {
    id: string;
    username: string;
    display_name: string;
    platform_admin: boolean;
    bound_school_id: string | null;
    grants: Grant[];
    wecom_authenticated: boolean;
}
export interface Session {
    user: User;
    csrf: string;
}
export interface School extends Row {
    name: string;
    active: boolean;
    revision: number;
    config: {
        [key: string]: Json;
    };
}
export interface Post extends Row {
    school_id: string;
    author_id: string;
    title: string;
    content: string;
    images: string[];
    tags: string[];
    audit_status: 'pending' | 'approved' | 'rejected';
    visibility: 'visible' | 'hidden' | 'deleted';
    revision: number;
}
export interface Draft extends Row {
    school_id: string;
    group_id: string;
    template_version_id: string;
    content: string;
    content_hash: string;
    state: 'ready' | 'invalidated';
    chat_id: string;
    group_name: string;
}
export type ClientState = 'client_reported_success' | 'client_reported_failure' | 'cancelled' | 'unknown';
export interface Attempt extends Row {
    school_id: string;
    draft_id: string;
    actor_id: string;
    state: 'prepared' | ClientState;
    reconciliation: 'operator_confirmed' | 'operator_rejected' | null;
    client_result: Json;
}
export interface RelatedHistory {
    items: Attempt[];
    page: PageMeta;
    latest_attempt_id: string | null;
    blocked: boolean;
}
export interface Signature {
    timestamp: number;
    nonceStr: string;
    signature: string;
}
export type Resource = 'posts' | 'comments' | 'cases' | 'resources' | 'groups' | 'templates' | 'delivery-drafts' | 'delivery-attempts' | 'audit' | 'members' | 'follows' | 'notifications' | 'reactions';
const part = (s: string) => encodeURIComponent(s);
export const scope = (school: string, path: string) => `/api/v1/schools/${part(school)}/${path}`;
export const identity = {
    async session() { const v = await requestResource<Session>('/api/v1/auth/session'); setCsrf(v.csrf); return v; },
    async login(username: string, password: string) { const v = await requestResource<Session>('/api/v1/auth/sessions', 'POST', { username, password }); setCsrf(v.csrf); return v; },
    async logout() { await requestResource('/api/v1/auth/session', 'DELETE'); setCsrf(''); },
    profile: (display_name: string) => requestResource('/api/v1/profile', 'PUT', { display_name }),
    accounts: () => requestResource<Row[]>('/api/v1/accounts'),
    create: (body: {
        username: string;
        display_name: string;
        password: string;
    }) => requestResource<Row>('/api/v1/accounts', 'POST', body),
    update: (id: string, body: {display_name: string; active: boolean}) => requestResource(`/api/v1/accounts/${part(id)}`, 'PUT', body),
    reset: (id: string, password: string) => requestResource(`/api/v1/accounts/${part(id)}/password`, 'POST', { username: '', password }),
};
export interface WecomStatus { configured: boolean; oauth_available: boolean }
export const workspace = {
    wecomStatus: () => requestResource<WecomStatus>('/api/v1/wecom/status'),
    globalResources: () => requestResource<Row[]>('/api/v1/platform/resources'),
    globalResource: (body: unknown) => requestResource('/api/v1/platform/resources', 'POST', body),
    schools: () => requestResource<School[]>('/api/v1/schools'),
    directory: () => requestResource<Pick<School, 'id' | 'name'>[]>('/api/v1/school-directory'),
    createSchool: (name: string) => requestResource<School>('/api/v1/schools', 'POST', { name }),
    updateSchool: (s: string, body: unknown) => requestResource<School>(`/api/v1/schools/${part(s)}`, 'PUT', body),
    bind: (s: string) => requestResource(scope(s, 'bind'), 'POST'),
    list: <T extends Row = Row>(s: string, resource: Resource, query = '', signal?: AbortSignal) => requestCollection<T>(scope(s, resource) + query, signal),
    getDraft: (s: string, id: string) => requestResource<Draft>(scope(s, `delivery-drafts/${part(id)}`)),
    async relatedHistory(s: string, id: string, page = 1): Promise<RelatedHistory> {
        const document = await requestDocument(scope(s, `delivery-drafts/${part(id)}/history`) + `?page[number]=${page}&page[size]=30`);
        const meta = document.meta;
        if (!Array.isArray(document.data) || !meta?.page || typeof meta.blocked !== 'boolean' || !(meta.latest_attempt_id === null || typeof meta.latest_attempt_id === 'string')) throw new Error('不合法的发送历史文档');
        return {items: decodeData<Attempt[]>(document), page: meta.page, blocked: meta.blocked, latest_attempt_id: meta.latest_attempt_id};
    },
    getPost: (s: string, id: string) => requestResource<Post>(scope(s, `posts/${part(id)}`)),
    comments: (s: string, id: string, page = 1) => requestCollection<Row>(scope(s, `posts/${part(id)}/comments`) + `?page[number]=${page}&page[size]=30`),
    mutate: <T = Row>(s: string, path: string, body?: unknown, method = 'POST') => requestResource<T>(scope(s, path), method, body),
    upload: (s: string, file: Blob) => requestResource<{
        url: string;
    }>(scope(s, 'media'), 'POST', file),
    prepare: (s: string, id: string, key: string, body: {
        current_chat_id: string;
        resend_of?: string;
        reason: string;
    }) => requestResource<Attempt>(scope(s, `delivery-drafts/${part(id)}/attempts`), 'POST', body, undefined, { 'Idempotency-Key': key }),
    claim: (s: string, id: string) => requestResource(scope(s, `delivery-attempts/${part(id)}/invocation`), 'POST'),
    result: (s: string, id: string, state: ClientState) => requestResource<Attempt>(scope(s, `delivery-attempts/${part(id)}/client-result`), 'PUT', { state }),
    wecomInfo: () => requestResource<{
        corpId: string;
        agentId: string;
    }>('/api/v1/wecom/config'),
    signature: (url: string, agent: boolean) => requestResource<Signature>(`/api/v1/wecom/js-sdk-config?${new URLSearchParams({ url, agent: String(agent) })}`),
};

/** OAuth only accepts UUID selections; arbitrary return URLs are not supported. */
export function wecomOAuthUrl(selection: { school?: string; draft?: string } = {}): string {
    const params = new URLSearchParams();
    if (selection.school) params.set('school', selection.school);
    if (selection.draft) params.set('draft', selection.draft);
    return `/api/v1/wecom/oauth/start${params.size ? `?${params}` : ''}`;
}

export interface DirectoryDepartment { id: number; parentid: number; name: string; order: number }
export interface DirectoryMember {
    id: string; userid: string; name: string; status: number | null;
    visibility: 'visible' | 'not_seen'; verification: 'verified' | 'unavailable' | 'unverified';
    departments: DirectoryDepartment[]; checked_at: string | null;
    bound_account_id: string | null; bound_account_name: string | null;
}
export interface DirectoryRun {
    id: string; state: 'queued' | 'running' | 'succeeded' | 'failed';
    created_at: string; started_at: string | null; finished_at: string | null;
    error_code: string | null; department_count: number; member_count: number;
}
export interface DirectoryStatus {
    id: string; configured: boolean; configuration_error: string | null;
    snapshot_id: string | null; last_success_at: string | null; latest_run: DirectoryRun | null;
}
export interface DirectoryFilter { page?: number; search?: string; department_id?: number; status?: number; binding?: 'bound' | 'unbound' }
const directoryPath = '/api/v1/platform/wecom-directory';
export const directory = {
    status: () => requestResource<DirectoryStatus>(`${directoryPath}/status`),
    members: (filter: DirectoryFilter = {}) => {
        const q = new URLSearchParams({'page[number]': String(filter.page ?? 1), 'page[size]': '30'});
        for (const key of ['search','department_id','status','binding'] as const) if (filter[key] !== undefined && filter[key] !== '') q.set(key, String(filter[key]));
        return requestCollection<DirectoryMember>(`${directoryPath}/members?${q}`);
    },
    async departments() {
        const items: DirectoryDepartment[] = [];
        for (let page = 1; ; page++) {
            const result = await requestCollection<Omit<DirectoryDepartment,'id'> & {id:string}>(`${directoryPath}/departments?page[number]=${page}&page[size]=100`);
            items.push(...result.items.map(d => ({...d,id:Number(d.id)})));
            if (!result.page?.has_more) return items;
        }
    },
    sync: () => requestResource<DirectoryRun>(`${directoryPath}/sync-runs`, 'POST'),
    run: (id: string) => requestResource<DirectoryRun>(`${directoryPath}/sync-runs/${part(id)}`),
    bind: (account: string, member_id: string, revision: number) => requestResource(`/api/v1/accounts/${part(account)}/wecom-binding`, 'PUT', {member_id, revision}),
    unbind: (account: string, revision: number) => requestResource(`/api/v1/accounts/${part(account)}/wecom-binding`, 'DELETE', undefined, undefined, {'If-Match': `"${revision}"`}),
};

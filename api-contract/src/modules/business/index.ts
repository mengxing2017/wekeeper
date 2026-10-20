import { requestJson, setCsrf } from '../../http';
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
export interface Signature {
    timestamp: number;
    nonceStr: string;
    signature: string;
}
export type Resource = 'posts' | 'comments' | 'cases' | 'resources' | 'groups' | 'templates' | 'delivery-drafts' | 'delivery-attempts' | 'audit' | 'members' | 'follows' | 'notifications' | 'reactions';
const part = (s: string) => encodeURIComponent(s);
export const scope = (school: string, path: string) => `/api/v1/schools/${part(school)}/${path}`;
async function data<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal, headers?: Record<string, string>): Promise<T> { return (await requestJson<{
    data: T;
}>(path, method, body, signal, headers)).data; }
export const identity = {
    async session() { const v = await data<Session>('/api/v1/auth/session'); setCsrf(v.csrf); return v; },
    async login(username: string, password: string) { const v = await data<Session>('/api/v1/auth/sessions', 'POST', { username, password }); setCsrf(v.csrf); return v; },
    async logout() { await data('/api/v1/auth/session', 'DELETE'); setCsrf(''); },
    profile: (display_name: string) => data('/api/v1/profile', 'PUT', { display_name }),
    accounts: () => data<Row[]>('/api/v1/accounts'),
    create: (body: {
        username: string;
        display_name: string;
        password: string;
    }) => data<Row>('/api/v1/accounts', 'POST', body),
    update: (id: string, body: unknown) => data(`/api/v1/accounts/${part(id)}`, 'PUT', body),
    reset: (id: string, password: string) => data(`/api/v1/accounts/${part(id)}/password`, 'POST', { username: '', password }),
};
export const workspace = {
    globalResources: () => data<Row[]>('/api/v1/platform/resources'),
    globalResource: (body: unknown) => data('/api/v1/platform/resources', 'POST', body),
    schools: () => data<School[]>('/api/v1/schools'),
    directory: () => data<Pick<School, 'id' | 'name'>[]>('/api/v1/school-directory'),
    createSchool: (name: string) => data<School>('/api/v1/schools', 'POST', { name }),
    updateSchool: (s: string, body: unknown) => data<School>(`/api/v1/schools/${part(s)}`, 'PUT', body),
    bind: (s: string) => data(scope(s, 'bind'), 'POST'),
    list: <T extends Row = Row>(s: string, resource: Resource, query = '', signal?: AbortSignal) => data<T[]>(scope(s, resource) + query, 'GET', undefined, signal),
    getPost: (s: string, id: string) => data<Post>(scope(s, `posts/${part(id)}`)),
    comments: (s: string, id: string) => data<Row[]>(scope(s, `posts/${part(id)}/comments`)),
    mutate: <T = Row>(s: string, path: string, body?: unknown, method = 'POST') => data<T>(scope(s, path), method, body),
    upload: (s: string, file: Blob) => data<{
        url: string;
    }>(scope(s, 'media'), 'POST', file),
    prepare: (s: string, id: string, key: string, body: {
        current_chat_id: string;
        resend_of?: string;
        reason: string;
    }) => data<Attempt>(scope(s, `delivery-drafts/${part(id)}/attempts`), 'POST', body, undefined, { 'Idempotency-Key': key }),
    claim: (s: string, id: string) => data(scope(s, `delivery-attempts/${part(id)}/invocation`), 'POST'),
    result: (s: string, id: string, state: ClientState) => data<Attempt>(scope(s, `delivery-attempts/${part(id)}/client-result`), 'PUT', { state }),
    wecomInfo: () => data<{
        corpId: string;
        agentId: string;
    }>('/api/v1/wecom/config'),
    signature: (url: string, agent: boolean) => data<Signature>(`/api/v1/wecom/js-sdk-config?${new URLSearchParams({ url, agent: String(agent) })}`),
};

/** OAuth only accepts UUID selections; arbitrary return URLs are not supported. */
export function wecomOAuthUrl(selection: { school?: string; draft?: string } = {}): string {
    const params = new URLSearchParams();
    if (selection.school) params.set('school', selection.school);
    if (selection.draft) params.set('draft', selection.draft);
    return `/api/v1/wecom/oauth/start${params.size ? `?${params}` : ''}`;
}

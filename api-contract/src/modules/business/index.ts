import { requestResource, requestCollection, setCsrf } from '../../http';
export { ApiError } from '../../http';
export * from './types'
import type { Row, Session, School, Post, Resource } from './types'
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
export interface WecomStatus { server_configured: boolean; client_configured: boolean; oauth_available: boolean; share_available: boolean }
export const workspace = {
    wecomStatus: () => requestResource<WecomStatus>('/api/v1/wecom/status'),
    globalResources: () => requestResource<Row[]>('/api/v1/platform/resources'),
    schools: () => requestResource<School[]>('/api/v1/schools'),
    directory: () => requestResource<Pick<School, 'id' | 'name'>[]>('/api/v1/school-directory'),
    createSchool: (name: string) => requestResource<School>('/api/v1/schools', 'POST', { name }),
    updateSchool: (s: string, body: unknown) => requestResource<School>(`/api/v1/schools/${part(s)}`, 'PUT', body),
    bind: (s: string) => requestResource(scope(s, 'bind'), 'POST'),
    list: <T extends Row = Row>(s: string, resource: Resource, query = '', signal?: AbortSignal) => requestCollection<T>(scope(s, resource) + query, signal),
    getPost: (s: string, id: string) => requestResource<Post>(scope(s, `posts/${part(id)}`)),
    comments: (s: string, id: string, page = 1) => requestCollection<Row>(scope(s, `posts/${part(id)}/comments`) + `?page[number]=${page}&page[size]=30`),
    mutate: <T = Row>(s: string, path: string, body?: unknown, method = 'POST') => requestResource<T>(scope(s, path), method, body),
    upload: (s: string, file: Blob) => requestResource<{
        url: string;
    }>(scope(s, 'media'), 'POST', file),

};

/** OAuth only accepts UUID selections; arbitrary return URLs are not supported. */
export function wecomOAuthUrl(selection: { school?: string } = {}): string {
    const params = new URLSearchParams();
    if (selection.school) params.set('school', selection.school);
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
    id: string; server_configured: boolean; configuration_error: string | null;
    credential_generation: number | null; credential_rotation_required: boolean;
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
    rotate: (expected_generation: number) => requestResource(`${directoryPath}/credential-rotations`, 'POST', {expected_generation}),
    sync: () => requestResource<DirectoryRun>(`${directoryPath}/sync-runs`, 'POST'),
    run: (id: string) => requestResource<DirectoryRun>(`${directoryPath}/sync-runs/${part(id)}`),
    bind: (account: string, member_id: string, revision: number) => requestResource(`/api/v1/accounts/${part(account)}/wecom-binding`, 'PUT', {member_id, revision}),
    unbind: (account: string, revision: number) => requestResource(`/api/v1/accounts/${part(account)}/wecom-binding`, 'DELETE', undefined, undefined, {'If-Match': `"${revision}"`}),
};

export type AssetDataset = 'groups';
export interface AssetRun extends Row {
    dataset: AssetDataset; generation: number; state: 'queued'|'running'|'succeeded'|'failed'; stage: string;
    processed: number; total: number; pages: number; error_code: string|null; complete: boolean; scope_changed: boolean;
    created_at: string; started_at: string|null; finished_at: string|null;
}
export interface AssetState {dataset:AssetDataset; current_run_id:string|null; last_success_at:string|null; published_generation:number|null; freshness:string; scope_changed:boolean; scope_count:number; latest_run:AssetRun|null}
export interface AssetStatus {server_configured:boolean; configuration_error:string|null; credential_generation:number|null; datasets:AssetState[]}
export interface AssetRecord extends Row {checked_at:string; credential_generation:number; visibility:'visible'|'not_seen'; verification:'verified'|'unverified'|'unavailable'}
export interface GroupCandidate {already_associated:boolean;id:string; name:string|null; chat_id:string; corp_id:string; owner:string; owner_name:string|null; member_count:number; credential_generation:number; checked_at:string}
export interface AssetFilter {page?:number; search?:string; member_id?:string; owner_member_id?:string; participant_member_id?:string; verification?:string; associated?:boolean; dataset?:AssetDataset}
export type AssetList = 'groups'|'member-statistics'|'sync-runs';
function assetQuery(filter:AssetFilter) {const q=new URLSearchParams({'page[number]':String(filter.page??1),'page[size]':'30'});for(const [key,value] of Object.entries(filter))if(key!=='page'&&value!==undefined&&value!=='')q.set(key,String(value));return q;}
const assetsPath='/api/v1/platform/wecom-assets';
export const assets = {
    confirmOwnerScope:(input:{run_id:string;owner_userid:string;expected_generation:number;reason:string})=>requestResource(`${assetsPath}/owner-scope-decisions`,'POST',input),
    status:()=>requestResource<AssetStatus>(`${assetsPath}/status`),
    sync:(dataset:AssetDataset)=>requestResource<AssetRun[]>(`${assetsPath}/sync-runs`,'POST',{dataset}),
    list:(kind:AssetList,filter:AssetFilter={})=>requestCollection<Row>(`${assetsPath}/${kind}?${assetQuery(filter)}`),
    detail:(kind:'groups'|'sync-runs',id:string)=>requestResource<Row>(`${assetsPath}/${kind}/${part(id)}`),
    relations:(kind:'groups',id:string,page=1)=>requestCollection<AssetRecord>(`${assetsPath}/${kind}/${part(id)}/members?${assetQuery({page})}`),
    candidates:(school:string,filter:Pick<AssetFilter,'page'|'search'>={})=>requestCollection<GroupCandidate>(scope(school,'wecom-group-candidates')+`?${assetQuery(filter)}`),
    bind:(school:string,input:{directory_group_id:string;expected_generation:number;expected_checked_at:string;name?:string})=>requestResource<Row>(scope(school,'groups/from-directory'),'POST',input),
};

// Transitional export only; the official contract has one canonical owner.
export * from '../wecom-distribution'

export * from '../product-workflows'

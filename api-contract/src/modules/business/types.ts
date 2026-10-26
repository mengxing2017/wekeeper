import type { PageMeta } from '../../http/protocol'
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
export type Resource = 'posts' | 'comments' | 'cases' | 'resources' | 'groups' | 'templates' | 'delivery-drafts' | 'delivery-attempts' | 'audit' | 'members' | 'follows' | 'notifications' | 'reactions';

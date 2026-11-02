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
export type Resource = 'posts' | 'comments' | 'cases' | 'resources' | 'groups' | 'templates' | 'audit' | 'members' | 'follows' | 'notifications' | 'reactions';

export type Json = string | number | boolean | null | Json[] | {
    [key: string]: Json;
};
export interface Row {
    id: string;
    [key: string]: Json;
}
export interface User {
    id: string;
    username: string;
    display_name: string;
    platform_admin: boolean;
    wecom_authenticated: boolean;
}
export interface Session {
    user: User;
    csrf: string;
}
export interface DistributionMaterial extends Row { title: string; content: string; version: string }
export type Resource = 'distribution-materials' | 'groups' | 'templates';

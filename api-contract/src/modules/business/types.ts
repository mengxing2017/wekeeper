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
export type Resource = 'groups';

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

export interface Administrator { id: string; username: string; display_name: string; active: boolean; platform_admin: boolean }

import type {Client} from '../../http/client'
import type {Session,Administrator} from './types'
export type {Session,User,Administrator} from './types'
const part=(value:string)=>encodeURIComponent(value)
export function createIdentityClient(client:Pick<Client,'requestResource'|'setCsrf'>) {
  return {
    async session() { const v = await client.requestResource<Session>('/api/v1/auth/session'); client.setCsrf(v?.csrf??''); return v; },
    async login(username: string, password: string) { const v = await client.requestResource<Session>('/api/v1/auth/sessions', 'POST', { username, password }); client.setCsrf(v.csrf); return v; },
    async logout() { await client.requestResource('/api/v1/auth/session', 'DELETE'); client.setCsrf(''); },
    profile: (display_name: string) => client.requestResource('/api/v1/profile', 'PUT', { display_name }),
    administrators: (search = '') => client.requestResource<Administrator[]>(`/api/v1/platform/administrators${search ? `?search=${encodeURIComponent(search)}` : ''}`),
    createAdministrator: (input: {username: string; display_name: string; password: string}) => client.requestResource<Administrator>('/api/v1/platform/administrators', 'POST', input),
    updateAdministrator: (id: string, input: {display_name: string; active: boolean}) => client.requestResource<Administrator>(`/api/v1/platform/administrators/${part(id)}`, 'PUT', input),
    deleteAdministrator: (id: string) => client.requestResource<null>(`/api/v1/platform/administrators/${part(id)}`, 'DELETE'),
    resetAdministrator: (id: string, password: string) => client.requestResource(`/api/v1/platform/administrators/${part(id)}/password`, 'POST', { password }),
};
}

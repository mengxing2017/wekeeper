import type { Client } from '../../http/client'
export type { User, Session } from '../business/types'
import type { User, Session } from '../business/types'
export interface MiniappSession {
  id: string
  token: string
  user: User
}
export function createIdentityClient(client: Client) {
  return {
    miniapp: (code: string) =>
      client.requestResource<MiniappSession>('/api/v1/auth/miniapp', 'POST', {
        code,
      }),
    session: () => client.requestResource<Session>('/api/v1/auth/session'),
    logout: () => client.requestResource<null>('/api/v1/auth/session', 'DELETE'),
    profile: (display_name: string) =>
      client.requestResource<{ id: string; display_name: string }>('/api/v1/profile', 'PUT', {
        display_name,
      }),
  }
}

import type { Client } from '../../http/client'
import type { School } from '../business/types'
export type { School } from '../business/types'
export const schoolPath = (school: string) => `/api/v1/schools/${encodeURIComponent(school)}`
export function createSchoolsClient(client: Client) {
  return {
    list: () => client.requestResource<School[]>('/api/v1/schools'),
    directory: () =>
      client.requestResource<Pick<School, 'id' | 'name'>[]>('/api/v1/school-directory'),
    bind: (school: string) => client.requestResource<null>(`${schoolPath(school)}/bind`, 'POST'),
  }
}

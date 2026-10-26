import { createSchoolsClient } from '@wekeeper/api-contract/schools'
import { client } from '../../identity/api'
export type { School } from '@wekeeper/api-contract/schools'
export const schoolsApi = createSchoolsClient(client)

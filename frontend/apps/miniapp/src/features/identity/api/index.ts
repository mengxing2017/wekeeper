import { createClient } from '@wekeeper/api-contract/client'
import { createIdentityClient } from '@wekeeper/api-contract/identity'
import { createUniTransport } from '../../../app/transport'
export type { Transport, TransportRequest, Cancellation } from '@wekeeper/api-contract/client'
export { ApiError } from '@wekeeper/api-contract/client'
export type { User } from '@wekeeper/api-contract/identity'
export const client = createClient(createUniTransport(import.meta.env.VITE_API_BASE_URL ?? ''))
export const identityApi = createIdentityClient(client)

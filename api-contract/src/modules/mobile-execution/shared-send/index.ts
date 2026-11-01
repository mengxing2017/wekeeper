import { requestResource } from '../../../http'
import { createSharedSendClient } from './client'
export * from './client'
export const sharedSend = createSharedSendClient({ requestResource })

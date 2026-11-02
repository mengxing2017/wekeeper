import { requestResource } from '../../../http'
import { createSharedSendClient } from './client'
export * from './client'
export const sharedSend = createSharedSendClient({ requestResource })

export {createSharedSendV2Client} from './v2'
import {createSharedSendV2Client} from './v2'
export const sharedSendV2=createSharedSendV2Client({requestResource})

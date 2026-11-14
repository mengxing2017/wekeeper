import {requestResource} from '../../http'
import {createMessageClient} from './automation-client'
export * from './automation-client'
export const businessMessages=createMessageClient({requestResource})

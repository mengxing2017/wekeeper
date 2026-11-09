import {requestResource} from '../../http'
import {createAdvertisingClient} from './client'
export * from './client'
export const advertising=createAdvertisingClient({requestResource})

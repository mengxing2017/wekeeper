import {requestResource} from '../../http'
import {createBusinessDirectoryClient} from './client'
export * from './client'
export const businessDirectory=createBusinessDirectoryClient({requestResource})

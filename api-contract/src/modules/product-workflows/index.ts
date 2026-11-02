import {requestResource,requestCollection} from '../../http'
import {createProductClient} from './client'
export * from './client'
export const product=createProductClient({requestResource,requestCollection})

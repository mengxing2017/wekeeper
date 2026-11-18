import {requestResource,setCsrf} from '../../http'
import {createIdentityClient} from './index'
export type {User,Session,Administrator} from './types'
export const identity=createIdentityClient({requestResource,setCsrf})

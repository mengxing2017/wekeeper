// Browser facade; non-Web platforms inject their own client.
import {requestResource} from '../../http'
import {createSourceRoutingClient} from './routing-client'
export * from './routing-client'
export const sourceRouting=createSourceRoutingClient({requestResource})

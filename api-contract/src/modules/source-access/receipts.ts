import {requestResource} from '../../http'
import {createSourceReceiptsClient} from './receipts-client'
export * from './receipts-client'
export const sourceReceipts=createSourceReceiptsClient({requestResource})

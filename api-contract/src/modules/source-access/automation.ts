import {requestResource} from '../../http'
import {createSourceAutomationClient} from './automation-client'
export * from './automation-client'
export const sourceAutomation=createSourceAutomationClient({requestResource})

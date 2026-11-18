import {requestResource} from '../../http'
import {createMessageStatisticsClient} from './client'
export * from './client'
export const messageStatistics=createMessageStatisticsClient({requestResource})

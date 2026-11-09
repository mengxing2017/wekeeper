import {requestResource} from '../../http'
import {createKeywordRulesClient} from './keywords-client'
export * from './keywords-client'
export const keywordRules=createKeywordRulesClient({requestResource})

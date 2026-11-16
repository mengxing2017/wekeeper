import type {Client} from '../../http/client'
export interface KeywordEntry {word:string;enabled:boolean;normalized?:string}
export interface SensitiveWords {revision:number;filter_enabled:boolean;entries:KeywordEntry[]}
export interface SensitiveWordsUpdate {expected_revision:number;filter_enabled:boolean;entries:KeywordEntry[]}
const words='/api/v2/admin/sensitive-words'
export function createKeywordRulesClient({requestResource}:Pick<Client,'requestResource'>){return {
 list:()=>requestResource<SensitiveWords>(words),
 save:(input:SensitiveWordsUpdate)=>requestResource<{revision:number}>(words,'PUT',input),
}}

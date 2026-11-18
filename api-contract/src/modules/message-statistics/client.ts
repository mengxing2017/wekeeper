import type {Client,Cancellation} from '../../http/client'
import type {StatisticsFilter,StatisticsOverview,DetailFilter,DetailPage,Breakdown,StatisticsExport} from './protocol'
export * from './protocol'
const base='/api/v2/admin/message-statistics/queries'
export function createMessageStatisticsClient({requestResource}:Pick<Client,'requestResource'>) {
 return {
  create:(filter:StatisticsFilter,signal?:Cancellation)=>requestResource<StatisticsOverview>(base,'POST',filter,signal),
  get:(id:string,signal?:Cancellation)=>requestResource<StatisticsOverview>(`${base}/${encodeURIComponent(id)}`,'GET',undefined,signal),
  details:(id:string,filter:DetailFilter,signal?:Cancellation)=>{
   const q=new URLSearchParams();for(const [k,v] of Object.entries(filter))if(v!==undefined)q.set(k,String(v))
   return requestResource<DetailPage>(`${base}/${encodeURIComponent(id)}/details?${q}`,'GET',undefined,signal)
  },
  breakdowns:(id:string,dimension:'unit'|'channel',page=1,signal?:Cancellation)=>requestResource<{items:Breakdown[];has_more:boolean}>(`${base}/${encodeURIComponent(id)}/breakdowns?dimension=${dimension}&page=${page}`,'GET',undefined,signal),
  export:(id:string)=>requestResource<StatisticsExport>(`${base}/${encodeURIComponent(id)}/exports`,'POST',{}),
 }
}

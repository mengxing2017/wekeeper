import type { MobileUsage } from './api'
export type UsageRow = MobileUsage & { history:MobileUsage[] }
export function usageRows(records:MobileUsage[]):UsageRow[] {
 const families=new Map<string,MobileUsage[]>()
 for(const row of records){const key=row.budget_root??row.id;families.set(key,[...(families.get(key)??[]),row])}
 return [...families.values()].map(family=>{
  const [current,...history]=family.sort((a,b)=>Date.parse(b.created_at)-Date.parse(a.created_at)||b.id.localeCompare(a.id))
  return {...current,history}
 }).sort((a,b)=>Date.parse(b.created_at)-Date.parse(a.created_at)||b.id.localeCompare(a.id))
}
export function localExpiry(value:string|number) {
 const date=new Date(value)
 return new Date(date.getTime()-date.getTimezoneOffset()*60000).toISOString().slice(0,-1)
}
export function openingExpiry(previous?:string, now=Date.now()) {
 const proposed=Math.max(now,previous?Date.parse(previous):0)+30*86400000
 // The API permits less than 366 days. Near the limit, renew later.
 return localExpiry(Math.min(proposed,now+365.5*86400000))
}

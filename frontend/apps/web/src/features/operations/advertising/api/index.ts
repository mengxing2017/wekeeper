export * from '@wekeeper/api-contract/advertising'
import {businessDirectory, type DirectoryPage} from '@wekeeper/api-contract/business-directory'
export async function advertisingScopes() {
 const all = async <T>(load: (page:number)=>Promise<DirectoryPage<T>>) => {
  const items:T[]=[]
  for(let page=1;;page++) {const result=await load(page);items.push(...result.items);if(!result.has_more)return items}
 }
 const platforms=await all(businessDirectory.platforms)
 const units=(await Promise.all(platforms.map(p=>all(page=>businessDirectory.units(p.id,page))))).flat()
 return {platforms,units}
}

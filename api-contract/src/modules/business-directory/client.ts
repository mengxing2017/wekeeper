import type {Client} from '../../http/client'

export interface BusinessPlatform {id:string;name:string;status:string}
export interface BusinessUnit {id:string;platform_id:string;name:string;active:boolean;revision:number}
export interface DirectoryPage<T> {items:T[];has_more:boolean}

/** System administrator directory; the transport is supplied by the host. */
export function createBusinessDirectoryClient({requestResource}:Pick<Client,'requestResource'>) {
 return {
  platforms:(page=1,size=30)=>requestResource<DirectoryPage<BusinessPlatform>>(`/api/v2/admin/business-platforms?page%5Bnumber%5D=${page}&page%5Bsize%5D=${size}`),
  units:(platformId:string,page=1,size=30)=>requestResource<DirectoryPage<BusinessUnit>>(`/api/v2/admin/business-platforms/${encodeURIComponent(platformId)}/units?page%5Bnumber%5D=${page}&page%5Bsize%5D=${size}`),
 }
}

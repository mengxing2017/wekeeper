import type {Client} from '../../http/client'
export interface RouteConfiguration {id:string;business_unit_id:string;name:string;status:string}
const routePath=(id:string)=>`/api/v2/admin/source-routes/${encodeURIComponent(id)}`
export function createSourceRoutingClient({requestResource}:Pick<Client,'requestResource'>){return {
 assigned:()=>requestResource<{items:{id:string;route_name:string;caller_name:string;business_unit_id:string;status:string}[]}>('/api/v2/admin/source-route-assignments'),
 configuration:(route:string)=>requestResource<RouteConfiguration>(`${routePath(route)}/configuration`),
}}

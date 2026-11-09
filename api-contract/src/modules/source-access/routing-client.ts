import type {ExecutionCapability} from '../execution-evidence'
import type {Client} from '../../http/client'
export type RoutingChannel='wecom'|'mobile'
export type RoutingEntityKind='routing-units'|'routing-groups'
export interface RoutingEntity {id:string;name:string;description:string;active:boolean;revision:number;unit_id?:string;created_at:string;updated_at:string}
export interface RoutingEntityInput {name:string;description:string;active:boolean;revision?:number;unit_id?:string}
export interface RoutingExecution {channel:RoutingChannel;workspace_id:string;target_ids:string[]}
export interface RoutingTarget {id:string;name:string;identity:Record<string,unknown>;resource_available:boolean}
export interface RoutingSnapshot {route_id:string;revision:number;source_id:string;unit_id:string;unit_name:string;group_id:string;group_name:string;execution:RoutingExecution|null;targets:RoutingTarget[];configured_by:string}
export interface RouteManager {id:string;username:string;revision:number;can_recheck:boolean}
export interface RouteConfiguration {execution_capability?:ExecutionCapability;id:string;revision:number;config_revision:number;unit_id:string|null;group_id:string|null;status:string;execution_state:'draft'|'running'|'paused';retired_at:string|null;snapshot:RoutingSnapshot|null;managers:RouteManager[]}
export interface RouteConfigurationInput {revision:number;unit_id:string;group_id:string;execution:RoutingExecution|null}
export interface RoutingCheck {ready:boolean;issues:string[]}
export interface RoutingRevision {revision:number;snapshot:RoutingSnapshot;actor_id:string;created_at:string}
export interface RoutingHandoff {id:string;route_id:string;receiver_path:string;authentication:string;signature_versions:number[];schema_versions:number[];contract_version:string;config_revision:number}
export type RoutingAction='enable'|'pause'|'resume'|'retire'
export interface RoutingMutation {id:string;revision:number;config_revision?:number;execution_state?:'running'|'paused'}
export interface RoutingGroupTarget {id:string;group_id:string;channel:RoutingChannel;workspace_id:string;target_id:string;added_at:string;removed_at:string|null}
const routePath=(id:string)=>`/api/v2/admin/source-routes/${encodeURIComponent(id)}`
export function createSourceRoutingClient({requestResource}:Pick<Client,'requestResource'>){return {
 assigned:()=>requestResource<{items:{id:string;route_name:string;caller_name:string;execution_state:'draft'|'running'|'paused';retired_at:string|null}[]}>('/api/v2/admin/source-route-assignments'),
 entities:(kind:RoutingEntityKind)=>requestResource<{items:RoutingEntity[]}>(`/api/v2/admin/routing/${kind}`),
 saveEntity:(kind:RoutingEntityKind,input:RoutingEntityInput,id?:string)=>requestResource<RoutingEntity>(`/api/v2/admin/routing/${kind}${id?`/${encodeURIComponent(id)}`:''}`,id?'PATCH':'POST',input),
 configuration:(route:string)=>requestResource<RouteConfiguration>(`${routePath(route)}/configuration`),
 configure:(route:string,input:RouteConfigurationInput)=>requestResource<RoutingMutation>(`${routePath(route)}/configuration`,'PUT',input),
 check:(route:string)=>requestResource<RoutingCheck>(`${routePath(route)}/checks`),
 lifecycle:(route:string,revision:number,action:RoutingAction,reason:string)=>requestResource<RoutingMutation>(`${routePath(route)}/lifecycle`,'POST',{revision,action,reason}),
 revisions:(route:string,page=1)=>requestResource<{items:RoutingRevision[];has_more:boolean}>(`${routePath(route)}/revisions?page=${page}`),
 manager:(route:string,revision:number,username:string,granted:boolean,canRecheck=false)=>requestResource<RoutingMutation>(`${routePath(route)}/managers`,'PUT',{revision,username,granted,can_recheck:canRecheck}),
 workspaces:(channel:RoutingChannel)=>requestResource<{items:{id:string;name:string}[]}>(`/api/v2/admin/source-route-resources?channel=${channel}`),
 targets:(channel:RoutingChannel,workspace:string)=>requestResource<{items:RoutingTarget[]}>(`/api/v2/admin/source-route-resources?channel=${channel}&workspace_id=${encodeURIComponent(workspace)}`),
 groupTargets:(group:string)=>requestResource<{items:RoutingGroupTarget[]}>(`/api/v2/admin/routing-groups/${encodeURIComponent(group)}/targets`),
 setGroupTargets:(group:string,revision:number,execution:RoutingExecution)=>requestResource<RoutingEntity>(`/api/v2/admin/routing-groups/${encodeURIComponent(group)}/targets`,'PUT',{revision,execution}),
 handoff:(route:string)=>requestResource<RoutingHandoff>(`${routePath(route)}/handoff`,'POST',{}),
}}
export const routingIssueLabels:Record<string,string>={route_configuration_required:'请补齐单位、分组、通道及目标配置',route_disabled:'接收已停用',route_retired:'路由已退役',source_authorization_revoked:'调用方凭据或路由授权无效',unit_disabled:'单位已停用',group_disabled:'分组已停用',route_no_targets:'没有有效目标群',workspace_authorization_revoked:'工作空间或配置者授权已失效',mobile_candidates_missing:'目标群缺少有效授权手机候选',channel_account_missing:'目标群缺少有效企业微信发送账号',route_paused:'自动执行已暂停'}

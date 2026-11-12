import type {Client} from '../../http/client'
import type {AdContent} from './client'

export interface ManagementPage<T> {items:T[];has_more:boolean}
export interface ScopedMaterial {id:string;revision:number;archived:boolean;content_version:string;content:AdContent}
export interface ScopedMaterialVersion {id:string;material_id:string;version:number;content:AdContent;created_by:string;created_at:string}
export interface CreateScopedMaterial {content:AdContent;reason:string}
export interface PublishScopedMaterial extends CreateScopedMaterial {expected_revision:number}
export interface ArchiveScopedMaterial {expected_revision:number;reason:string}
export type PlacementScope = {kind:'global'}|{kind:'platform';platform_id:string}|{kind:'unit';platform_id:string;unit_id:string}
export interface CreatePlacementRule {name:string;material_id:string;scope:PlacementScope;enabled:boolean;valid_from:string;valid_until:string|null;reason:string}
export interface PlacementRule {id:string;name:string;material_id:string;scope:PlacementScope;enabled:boolean;valid_from:string;valid_until:string|null;revision:number}
export interface SetPlacementState {expected_revision:number;enabled:boolean;reason:string}

/** Independent material management; the host supplies its own transport. */
export function createScopedMaterialClient({requestResource}:Pick<Client,'requestResource'>) {
 return {
  list:(page=1)=>requestResource<ManagementPage<ScopedMaterial>>(`/api/v2/admin/ad-materials?page=${page}`),
  create:(input:CreateScopedMaterial)=>requestResource<ScopedMaterial>('/api/v2/admin/ad-materials','POST',input),
  get:(id:string)=>requestResource<ScopedMaterial>(`/api/v2/admin/ad-materials/${encodeURIComponent(id)}`),
  versions:(id:string,page=1)=>requestResource<ManagementPage<ScopedMaterialVersion>>(`/api/v2/admin/ad-materials/${encodeURIComponent(id)}/versions?page=${page}`),
  archive:(id:string,input:ArchiveScopedMaterial)=>requestResource<ScopedMaterial>(`/api/v2/admin/ad-materials/${encodeURIComponent(id)}/archive`,'PUT',input),
  publish:(id:string,input:PublishScopedMaterial)=>requestResource<ScopedMaterial>(`/api/v2/admin/ad-materials/${encodeURIComponent(id)}/publications`,'POST',input),
 }
}

export function createPlacementRuleClient({requestResource}:Pick<Client,'requestResource'>) {
 return {
  list:(page=1)=>requestResource<ManagementPage<PlacementRule>>(`/api/v2/admin/ad-placement-rules?page=${page}`),
  create:(input:CreatePlacementRule)=>requestResource<PlacementRule>('/api/v2/admin/ad-placement-rules','POST',input),
  get:(id:string)=>requestResource<PlacementRule>(`/api/v2/admin/ad-placement-rules/${encodeURIComponent(id)}`),
  setState:(id:string,input:SetPlacementState)=>requestResource<PlacementRule>(`/api/v2/admin/ad-placement-rules/${encodeURIComponent(id)}/state`,'PUT',input),
 }
}

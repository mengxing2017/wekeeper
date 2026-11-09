import type {Client} from '../../http/client'
export type AdListFilter = 'all'|'owned'|'shared'|'archived'
export interface AdScope {kind:'unit'|'group'|'route';id:string;channel:'mobile'|'wecom'|null;workspace_id:string|null}
export interface AdContent {name:string;body:string;link:string|null}
export interface AdPolicy {enabled:boolean;valid_from:string;valid_until:string|null;allows:AdScope[];denies:AdScope[]}
export interface AdConfig {mode:'none'|'fixed'|'pool';fixed_ad_id:string|null;insertion_position:'before'|'after';unavailable_policy:'without_ad'|'configuration_error';auto_recover:boolean}
export interface Advertisement {id:string;owner_unit_id:string;owner_name:string;revision:number;content_version:string|null;policy_version:string|null;archived:boolean;content:AdContent;policy:AdPolicy|null;can_edit:boolean;can_manage_sharing:boolean;can_use:boolean;references?:{route_id:string;revision:number}[]}
export interface AdDirectory {units:{id:string;name:string;can_edit:boolean;can_share:boolean}[];scopes:{kind:AdScope['kind'];id:string;name:string}[];workspaces:{id:string;channel:'mobile'|'wecom';name:string}[];routes:{id:string;name:string}[];system_admin:boolean}
export interface AdManager {account_id:string;username:string;revision:number;can_edit:boolean;can_share:boolean}
export interface AdMeasurement {bytes:number;scalars:number;max_bytes:number;max_scalars:number|null;valid:boolean}
export interface AdSelection {pool?:AdPoolSelection;candidates:{ad_id:string;content_version:string|null;policy_version:string|null;exclusion:string|null}[];config:AdConfig;config_revision:number;ad_id:string|null;content_version:string|null;policy_version:string|null;composer:string;exclusions:string[];measurement:AdMeasurement}
export interface AdPoolSlot {slot_id:string;ad_id:string}
export interface AdPoolSlotInput {slot_id:string|null;ad_id:string}
export interface AdPoolPublication {expected_revision:number;reason:string;slots:AdPoolSlotInput[]}
export interface AdPoolReset {expected_revision:number;expected_cursor_version:number;reason:string}
export interface AdPoolCopy {target_route_id:string;expected_revision:number;reason:string}
export interface AdPoolCursor {pool_revision:number;cursor_version:number;reset_epoch:number;next_slot_id:string|null;last_consumed_slot_id:string|null;cycle:number;selection_order:number}
export interface AdPoolSelection {pool_id:string;before:AdPoolCursor;after:AdPoolCursor;selected_slot_id:string|null;selected_position:number|null;receive_order:number|null;rule_fingerprint:string;scans:{slot_id:string;position:number;ad_id:string;exclusion:string|null}[]}
export interface AdPool {id:string;pool_id:string|null;revision:number;slots:AdPoolSlot[];cursor:AdPoolCursor|null;transitions:{id:string;kind:string;actor_id:string;reason:string;before_cursor:AdPoolCursor;after_cursor:AdPoolCursor;created_at:string}[]}
export interface AdPoolStatistics {selections:number;valid_targets:number;device_attempts:number;sent_targets:number;failed_targets:number;unknown_targets:number;pending_targets:number}
export interface AdPoolConsumption {id:string;pool_id:string;pool_revision:number;receipt_id:string;chain_decision_id:string;selection_order:number;slot_id:string;composition_id:string;selection:AdSelection;created_at:string}
export interface Composition {id:string;receipt_id:string;generation:number;supersedes:string|null;selection:AdSelection;content:string;content_digest:string;composer:string;message_snapshot_id:string;common_check:string;workspace_check:string;final_check:string;reason:string;created_at:string}
export interface AdVersion {kind:'content'|'policy';revision:number;value:{id:string;actor_id:string;reason:string;created_at:string;name?:string;body?:string;link?:string|null;policy?:AdPolicy}}
export interface PreviewCheck {total_hits:number;matched_rules:{word:string;scope_id:string}[];input_digest:string;normalized_digest:string}
export interface CompositionPreview {content:string|null;selection:AdSelection|null;checks:{common:PreviewCheck;original:PreviewCheck|null;final:PreviewCheck|null;excluded_final:PreviewCheck|null};rule_fingerprint:string;preview_only:boolean;deliverable:boolean}
export interface AdMutation {id:string;revision:number;version_id?:string}
const base='/api/v2/admin/advertisements',ad=(id:string)=>`${base}/${encodeURIComponent(id)}`,route=(id:string)=>`/api/v2/admin/source-routes/${encodeURIComponent(id)}`
export function createAdvertisingClient({requestResource}:Pick<Client,'requestResource'>){return {
 pool:(id:string)=>requestResource<AdPool>(`${route(id)}/advertisement-pool`),
 publishPool:(id:string,input:AdPoolPublication)=>requestResource<AdPool>(`${route(id)}/advertisement-pool/publications`,'POST',input),
 resetPool:(id:string,input:AdPoolReset)=>requestResource<AdPool>(`${route(id)}/advertisement-pool/resets`,'POST',input),
 copyPool:(id:string,input:AdPoolCopy)=>requestResource<AdPool>(`${route(id)}/advertisement-pool/copies`,'POST',input),
 consumptions:(id:string,page=1)=>requestResource<{items:AdPoolConsumption[];has_more:boolean;statistics:AdPoolStatistics;slot_statistics:{slot_id:string;ad_id:string;selections:number}[];ad_statistics:{ad_id:string;selections:number}[]}>(`${route(id)}/advertisement-pool/consumptions?page=${page}`),
 directory:()=>requestResource<AdDirectory>('/api/v2/admin/advertisement-directory'),
 list:(page=1,filter:AdListFilter='all')=>requestResource<{items:Advertisement[];has_more:boolean}>(`${base}?page=${page}&filter=${encodeURIComponent(filter)}`),
 detail:(id:string)=>requestResource<Advertisement>(ad(id)),
 create:(input:{owner_unit_id:string;content:AdContent;workspaces:AdScope[];reason:string})=>requestResource<AdMutation>(base,'POST',input),
 content:(id:string,expected_revision:number,content:AdContent,reason:string)=>requestResource<AdMutation>(`${ad(id)}/content-publications`,'POST',{expected_revision,content,reason}),
 policy:(id:string,expected_revision:number,policy:AdPolicy,reason:string)=>requestResource<AdMutation>(`${ad(id)}/policy-publications`,'POST',{expected_revision,policy,reason}),
 archive:(id:string,expected_revision:number,reason:string)=>requestResource<AdMutation>(`${ad(id)}/archives`,'POST',{expected_revision,reason}),
 versions:(id:string,page=1)=>requestResource<{items:AdVersion[];has_more:boolean}>(`${ad(id)}/versions?page=${page}`),
 managers:(unit:string)=>requestResource<{items:AdManager[]}>(`/api/v2/admin/routing-units/${encodeURIComponent(unit)}/advertisement-managers`),
 manager:(unit:string,username:string,expected_revision:number,can_edit:boolean,can_share:boolean)=>requestResource<AdMutation>(`/api/v2/admin/routing-units/${encodeURIComponent(unit)}/advertisement-managers`,'PUT',{username,expected_revision,can_edit,can_share}),
 candidates:(id:string,page=1)=>requestResource<{items:Advertisement[];has_more:boolean}>(`${route(id)}/advertisement-candidates?page=${page}`),
 config:(id:string)=>requestResource<{id:string;revision:number;config:AdConfig}>(`${route(id)}/advertisement-configuration`),
 configure:(id:string,expected_revision:number,config:AdConfig,reason:string)=>requestResource<AdMutation>(`${route(id)}/advertisement-configuration`,'PUT',{expected_revision,config,reason}),
 preview:(id:string,input:{title:string|null;content:string|null;detail_url:string})=>requestResource<CompositionPreview>(`${route(id)}/composition-previews`,'POST',input),
 compositions:(id:string,receipt:string,page=1)=>requestResource<{items:Composition[];has_more:boolean;legacy:{content:string}|null;decisions:{id:string;selection:AdSelection;reason:string;composition_id:string|null;created_at:string}[];generations:{generation:number;task_id:string;composition_id:string|null}[]}>(`${route(id)}/receipts/${encodeURIComponent(receipt)}/compositions?page=${page}`),
}}

export const advertisementReasonLabels:Record<string,string>={ad_pool_no_candidate:'序列中无可用广告，未消费轮次',ad_disabled:'广告已停用',ad_not_started:'广告尚未生效',ad_expired:'广告已过期',ad_denied:'命中禁投范围',ad_not_allowed:'未获当前范围投放授权',ad_archived:'广告已归档',ad_unpublished:'广告尚未发布',ad_content_too_long:'组合正文不符合通道限制',ad_final_keyword_blocked:'广告组合命中最终关键字规则',ad_unavailable:'所选广告不可用，等待修复配置',ad_frozen_unavailable:'冻结广告失效，保持原正文并拦截未执行目标',selected:'已冻结组合'}

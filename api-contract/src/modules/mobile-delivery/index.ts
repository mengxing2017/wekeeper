import { requestResource, requestCollection, type Collection } from '../../http'
export type UUID = string
/** External system identifiers are unavailable to visible UI execution. These UUIDs are local only. */
export type IdentityMode = 'legacy_external' | 'ui_visible'
export type VerificationState = 'pending' | 'reviewed'
export interface AccountProfile { company_name: string; account_name: string; account_detail_text?: string | null }
export interface GroupProfile { group_type: 'external' | 'internal'; owner_name?: string | null }
export interface VisibleVerification { method: 'visible_ui'; account_profile_revision: number; group_profile_revision: number; evidence_ref: string; checks: Array<'company' | 'account' | 'group_name' | 'group_context'> }
export interface StrategyInput { name: string; sender_account_id: UUID; group_id: UUID; device_id: UUID; text_content: string; trigger_mode: 'manual_once'; enabled: boolean }
export interface DeliveryStrategy extends Row, StrategyInput { school_id: UUID; created_by: UUID; created_at: string; updated_at: string }
export interface StrategyRun { id: UUID; school_id: UUID; strategy_id: UUID; strategy_revision: number; task_id: UUID; device_id: UUID; device_binding_revision: number; strategy_snapshot: DeliveryStrategy; created_by: UUID; task_state?: MobileTask['state']; created_at: string }
export type AccountInput = { client_kind: 'wechat' | 'wecom'; identity_namespace: string; external_key: string; identity_mode?: 'legacy_external'; responsible_actor_id: UUID; identity_evidence_ref: string } | { client_kind: 'wecom'; identity_mode: 'ui_visible'; visible_profile: AccountProfile; responsible_actor_id: UUID; identity_evidence_ref: string }
export type GroupInput = { name: string; client_kind: 'wechat' | 'wecom'; identity_namespace: string; external_key: string; identity_mode?: 'legacy_external'; identity_evidence_ref: string; wecom_group_id: UUID | null } | { name: string; client_kind: 'wecom'; identity_mode: 'ui_visible'; visible_profile: GroupProfile; identity_evidence_ref: string; wecom_group_id?: UUID | null }
export interface Row { id: UUID; revision: number }
export interface Device extends Row { execution_protocol_version?: 1 | 2; name: string; enabled: boolean; credential_generation: number; client_instance_id: string | null; last_seen_at: string | null; created_at: string }
export interface SendingAccount extends Row { client_kind: 'wechat' | 'wecom'; identity_namespace: string | null; external_key: string | null; identity_mode?: IdentityMode; visible_profile?: AccountProfile | null; profile_revision?: number; verification_state?: VerificationState; responsible_actor_id: UUID; identity_evidence_ref: string; verified_at: string; enabled: boolean; daily_limit: number }
export interface DeliveryGroup extends Row { school_id: UUID; name: string; client_kind: 'wechat' | 'wecom'; identity_namespace: string | null; external_key: string | null; identity_mode?: IdentityMode; visible_profile?: GroupProfile | null; profile_revision?: number; verification_state?: VerificationState; identity_evidence_ref: string; wecom_group_id: UUID | null; enabled: boolean; daily_limit: number }
export type ContentSource = { kind: 'text'; text_content: string } | { kind: 'posts'; template_version_id: UUID; post_ids: UUID[] }
export interface ItemInput { group_id: UUID; sender_account_id: UUID; source: ContentSource }
export interface DraftInput { expected_revision?: number | null; activity_id: string; items: ItemInput[] }
export interface MobileTask extends Row { school_id: UUID; creator_id: UUID; confirmed_by: UUID | null; activity_id: string; state: 'draft' | 'confirmed' | 'completed' | 'stopped'; review_evidence_ref: string | null; created_at: string }
export type ItemState = 'queued' | 'claimed' | 'prepared' | 'permit_issued' | 'ui_observed_sent' | 'unknown' | 'manually_sent' | 'verified_not_sent' | 'cancelled'
export interface VisibleSnapshot { group?: { name?: string; visible_profile?: GroupProfile }; sender?: { visible_profile?: AccountProfile }; execution_target?: { device_id: UUID; device_binding_revision: number; strategy_id: UUID; strategy_revision: number; run_id: UUID }; [key: string]: unknown }
export interface TargetOptions { accounts_has_more: boolean; devices_has_more: boolean; id: UUID; accounts: SendingAccount[]; devices: Array<Device & { sender_account_id: UUID; binding_revision: number }> }
export interface MobileItem extends Row { school_id: UUID; task_id: UUID; group_id: UUID; sender_account_id: UUID; content: string; content_digest: string; source: ContentSource; snapshot: VisibleSnapshot; snapshot_digest: string; fence: number; state: ItemState; observed_at: string | null; created_at: string }
export interface Settings extends Row { school_id: UUID; enabled: boolean; history_reviewed: boolean; approved_client_version?: string; approved_script_version?: string; approved_baseline_version?: string; evidence_ref?: string }
export interface SettingsInput { expected_revision: number; enabled: boolean; history_reviewed: boolean; approved_client_version: string; approved_script_version: string; approved_baseline_version: string; evidence_ref: string }
export interface Capabilities { id: UUID; create: boolean; confirm: boolean; claim_enabled: boolean; reason: string | null; quota_summary: Array<{ group_id: UUID; daily_limit: number; used: number; unknown: number }>; quota_summary_limit: number; settings: Settings; minimum_protocol_version: number; account_default_limit: number; account_max_limit: number; group_default_limit: number; group_max_limit: number }
export interface Preview { id: UUID; expected_revision: number; preview_digest: string; requires_substantive_difference_review: boolean; items: Array<{ id: UUID; group_id: UUID; content: string; snapshot_digest: string; reason: string | null; next_eligible_at: string | null }> }
export interface DeliveryEvent { id: UUID; school_id: UUID | null; task_id: UUID | null; actor_id: UUID | null; device_id: UUID | null; action: string; detail: Record<string, unknown>; created_at: string }
export interface Enrollment { id: UUID; device_id: UUID; code?: string; expires_at: string; secret_replay_unavailable?: boolean }
export interface RevisionReason { expected_revision: number; reason: string }
export interface PolicyInput extends RevisionReason { daily_limit: number }
export interface ConfirmInput { expected_revision: number; preview_digest: string; content_review: boolean; scope_acceptance: boolean; substantive_difference_reviewed: boolean; evidence_ref: string }
export interface ReconcileInput extends RevisionReason { decision: 'manually_sent' | 'verified_not_sent'; evidence_ref: string }
const segment = (id: string) => encodeURIComponent(id)
const school = (s: UUID) => `/api/v1/schools/${segment(s)}`
const devices = '/api/v1/platform/execution-devices'
const accounts = '/api/v1/platform/sending-accounts'
function write<T>(path: string, method: string, input: unknown, key: string): Promise<T> {
  if (!key.trim() || key.length > 128) throw new Error('Idempotency-Key must contain 1–128 characters')
  return requestResource<T>(path, method, input, undefined, { 'Idempotency-Key': key })
}
function list<T>(path: string, page = 1, size = 30): Promise<Collection<T>> { return requestCollection<T>(`${path}?page[number]=${page}&page[size]=${size}`) }
export const mobileDeliveryApi = {
  updateAccountProfile: (id: UUID, input: { expected_revision: number; visible_profile: AccountProfile; reason: string }, key: string) => write<SendingAccount>(`${accounts}/${segment(id)}/visible-profile`, 'PATCH', input, key),
  reviewAccountProfile: (id: UUID, input: { expected_revision: number; evidence_ref: string }, key: string) => write<SendingAccount>(`${accounts}/${segment(id)}/visible-profile-reviews`, 'POST', input, key),
  updateGroupProfile: (s: UUID, id: UUID, input: { expected_revision: number; name: string; visible_profile: GroupProfile; reason: string }, key: string) => write<DeliveryGroup>(`${school(s)}/delivery-groups/${segment(id)}/visible-profile`, 'PATCH', input, key),
  reviewGroupProfile: (s: UUID, id: UUID, input: { expected_revision: number; evidence_ref: string }, key: string) => write<DeliveryGroup>(`${school(s)}/delivery-groups/${segment(id)}/visible-profile-reviews`, 'POST', input, key),
  targets: (s: UUID, page = 1) => requestResource<TargetOptions>(`${school(s)}/mobile-delivery-targets?page[number]=${page}&page[size]=100`),
  strategies: (s: UUID, page?: number, size?: number) => list<DeliveryStrategy>(`${school(s)}/mobile-delivery-strategies`, page, size),
  strategy: (s: UUID, id: UUID) => requestResource<DeliveryStrategy>(`${school(s)}/mobile-delivery-strategies/${segment(id)}`),
  createStrategy: (s: UUID, input: StrategyInput, key: string) => write<DeliveryStrategy>(`${school(s)}/mobile-delivery-strategies`, 'POST', input, key),
  updateStrategy: (s: UUID, id: UUID, input: StrategyInput & { expected_revision: number }, key: string) => write<DeliveryStrategy>(`${school(s)}/mobile-delivery-strategies/${segment(id)}`, 'PATCH', input, key),
  runStrategy: (s: UUID, id: UUID, input: { expected_revision: number }, key: string) => write<StrategyRun>(`${school(s)}/mobile-delivery-strategies/${segment(id)}/runs`, 'POST', input, key),
  strategyRuns: (s: UUID, id: UUID, page?: number, size?: number) => list<StrategyRun>(`${school(s)}/mobile-delivery-strategies/${segment(id)}/runs`, page, size),
  devices: (page?: number, size?: number) => list<Device>(devices, page, size),
  device: (id: UUID) => requestResource<Device>(`${devices}/${segment(id)}`),
  createDevice: (input: { name: string }, key: string) => write<Device>(devices, 'POST', input, key),
  updateDevice: (id: UUID, input: RevisionReason & { enabled: boolean }, key: string) => write<Device>(`${devices}/${segment(id)}`, 'PATCH', input, key),
  enrollment: (id: UUID, input: { expected_revision: number }, key: string) => write<Enrollment>(`${devices}/${segment(id)}/enrollments`, 'POST', input, key),
  revoke: (id: UUID, input: RevisionReason, key: string) => write<Device>(`${devices}/${segment(id)}/credential-revocations`, 'POST', input, key),
  accounts: (page?: number, size?: number) => list<SendingAccount>(accounts, page, size),
  createAccount: (input: AccountInput, key: string) => write<SendingAccount>(accounts, 'POST', input, key),
  bindDevice: (id: UUID, input: { expected_revision: number; sender_account_id: UUID; identity_evidence_ref: string }, key: string) => write<{ id: UUID; device_id: UUID; sender_account_id: UUID; revision: number; identity_evidence_ref: string }>(`${devices}/${segment(id)}/account-bindings`, 'POST', input, key),
  accountPolicy: (id: UUID) => requestResource<SendingAccount>(`${accounts}/${segment(id)}/delivery-policy`),
  updateAccountPolicy: (id: UUID, input: PolicyInput, key: string) => write<SendingAccount>(`${accounts}/${segment(id)}/delivery-policy`, 'PATCH', input, key),
  groups: (s: UUID, page?: number, size?: number) => list<DeliveryGroup>(`${school(s)}/delivery-groups`, page, size),
  createGroup: (s: UUID, input: GroupInput, key: string) => write<DeliveryGroup>(`${school(s)}/delivery-groups`, 'POST', input, key),
  bindGroup: (s: UUID, id: UUID, input: { sender_account_id: UUID; mapping_revision: number; reason: string }, key: string) => write<DeliveryGroup>(`${school(s)}/delivery-groups/${segment(id)}/sender-bindings`, 'POST', input, key),
  groupPolicy: (s: UUID, id: UUID) => requestResource<DeliveryGroup>(`${school(s)}/delivery-groups/${segment(id)}/delivery-policy`),
  updateGroupPolicy: (s: UUID, id: UUID, input: PolicyInput, key: string) => write<DeliveryGroup>(`${school(s)}/delivery-groups/${segment(id)}/delivery-policy`, 'PATCH', input, key),
  capabilities: (s: UUID) => requestResource<Capabilities>(`${school(s)}/mobile-delivery-capabilities`),
  settings: (s: UUID) => requestResource<Settings>(`${school(s)}/mobile-delivery-settings`),
  updateSettings: (s: UUID, input: SettingsInput, key: string) => write<Settings>(`${school(s)}/mobile-delivery-settings`, 'PATCH', input, key),
  tasks: (s: UUID, page?: number, size?: number) => list<MobileTask>(`${school(s)}/mobile-delivery-tasks`, page, size),
  task: (s: UUID, id: UUID) => requestResource<MobileTask>(`${school(s)}/mobile-delivery-tasks/${segment(id)}`),
  createTask: (s: UUID, input: DraftInput, key: string) => write<MobileTask>(`${school(s)}/mobile-delivery-tasks`, 'POST', input, key),
  editTask: (s: UUID, id: UUID, input: DraftInput & { expected_revision: number }, key: string) => write<MobileTask>(`${school(s)}/mobile-delivery-tasks/${segment(id)}`, 'PATCH', input, key),
  preview: (s: UUID, id: UUID, input: { expected_revision: number }, key: string) => write<Preview>(`${school(s)}/mobile-delivery-tasks/${segment(id)}/previews`, 'POST', input, key),
  confirm: (s: UUID, id: UUID, input: ConfirmInput, key: string) => write<MobileTask>(`${school(s)}/mobile-delivery-tasks/${segment(id)}/confirmations`, 'POST', input, key),
  stop: (s: UUID, id: UUID, input: RevisionReason, key: string) => write<MobileTask>(`${school(s)}/mobile-delivery-tasks/${segment(id)}/stops`, 'POST', input, key),
  items: (s: UUID, id: UUID, page?: number, size?: number) => list<MobileItem>(`${school(s)}/mobile-delivery-tasks/${segment(id)}/items`, page, size),
  events: (s: UUID, id: UUID, page?: number, size?: number) => list<DeliveryEvent>(`${school(s)}/mobile-delivery-tasks/${segment(id)}/events`, page, size),
  reconcile: (s: UUID, id: UUID, input: ReconcileInput, key: string) => write<MobileItem>(`${school(s)}/mobile-delivery-items/${segment(id)}/reconciliations`, 'POST', input, key),
  resendDraft: (s: UUID, id: UUID, input: RevisionReason, key: string) => write<MobileTask>(`${school(s)}/mobile-delivery-items/${segment(id)}/resend-drafts`, 'POST', input, key),
}

import type {ChannelObservation,ExecutionCapability} from '../execution-evidence'
export {executionEvidenceLabels} from '../execution-evidence'
export type {ChannelObservation,ExecutionCapability} from '../execution-evidence'
import type { Client } from '../../http/client';
import type { SourceReference } from '../distribution-content';
const part=(s:string)=>encodeURIComponent(s);
const scope=(school:string,path:string)=>`/api/v1/wecom-distribution/workspaces/${part(school)}/${path}`;
export type BroadcastObservation=Omit<ChannelObservation,'id'>&{id:string}
export interface BroadcastTask {
    id: string; school_id: string; creator_id: string; template_version_id: string|null; content_source: 'posts'|'text'|'materials'; source_namespace: string; materials: SourceReference[]; content: string;
    revision: number; state: 'draft'|'confirmed'|'abandoned'; stopped: boolean; created_at: string;
    total: number; sent: number; rejected: number; uncertain: number; pending: number; batch_count: number;
    post_ids: string[]; source_task_id: string|null; resend_reason: string; draft_input:BroadcastDraftInput;
}
export interface BroadcastTarget {
    execution_mode?:string;sync_failures?:number;next_sync_at?:string|null;evidence?:ChannelObservation|null;
    id: string; group_id: string; chat_id: string; group_name: string; batch_id: string; sender: string;
    creation_state: 'queued'|'submitting'|'created'|'rejected'|'unknown'|'cancelled'; msgid: string|null;
    rejected: boolean; official_status: number|null; send_time: number|null; member_status: number|null;
    last_synced_at: string|null; sync_error: string|null; error_code: string|null;
    stop_state: 'none'|'requested'|'stopped'|'unknown'|'rejected'; reconciliation: string|null; operation_unknown: boolean;
}
export type BroadcastDraftInput = {
    group_ids: string[]; revision?: number;
    source_task_id?: string; reason?: string;
} & (
    {content_source: 'posts'; template_version_id: string; post_ids: string[]; text_content?: never} |
    {content_source: 'text'; text_content: string; template_version_id?: null; post_ids?: never[]} |
    {content_source: 'materials'; materials: SourceReference[]; template_version_id?: null; post_ids?: never[]; text_content?: never}
);
export interface BroadcastAction {
    preview_digest?: string; revision?: number; batch_id?: string; decision?: 'not_created'|'sent'|'linked'|'stop_confirmed'|'reminder_reviewed'; evidence_id?:string; reason?: string;
}
export interface BroadcastEvent {id:string;actor_name:string;description:string;reason:string;created_at:string}
export interface BroadcastCapability {execution?:ExecutionCapability;id: string; broadcast_available: boolean; management_available: boolean; reason: string|null; reason_code?:string; health_state?:string;control?:{paused:boolean;revision:number}|null; can_edit_draft?:boolean; can_preview?:boolean; can_confirm?:boolean; can_refresh?:boolean; can_stop?:boolean; request?:import('../product-workflows/client').BusinessRequest|null}
const broadcastPath = (s: string, id?: string) => scope(s, `broadcast-tasks${id ? `/${part(id)}` : ''}`);
export interface BroadcastPreview {id:string; revision:number; digest:string; expires_at:string; content:string; target_count:number; channel:'wecom'}


export interface OfficialSender {
    id:string; school_id:string; userid:string; display_name:string;
    member_verified:boolean; member_active:boolean; account_active:boolean;
    binding_revision:number; qualification_revision:number; enabled:boolean;
    eligible:boolean; reason:string|null;
}

export interface SourcePolicy {namespace:string;enabled:boolean;max_pending:number;revision:number}
export interface ResourcePolicy {id:string;member_id:string|null;display_name:string;enabled:boolean;window_seconds:number;max_calls:number;revision:number}
export interface DistributionPolicies {sources:SourcePolicy[];resources:ResourcePolicy[]}


export function createWecomDistributionClient(client: Pick<Client,'requestResource'|'requestCollection'>) {
  const {requestResource,requestCollection}=client;
const broadcast = {
    savePreview:(s:string,input:BroadcastDraftInput,id:string|undefined,key:string)=>requestResource<{id:string;task_id:string;preview:BroadcastPreview|null;saved:boolean;reason:string|null}>(scope(s,'broadcast-task-previews'),'POST',{...input,id},undefined,{'Idempotency-Key':key}),
    evidenceCandidates:(s:string,id:string,batch:string)=>requestResource<{id:string;items:{id:string;summary:{content:string;sender:string;sender_name?:string|null;target_count:number;created_at:number|null;member_status:number};expires_at:string}[];reason:string}>(`${broadcastPath(s,id)}/evidence-candidates?batch_id=${part(batch)}`),
    preview: (s:string,id:string)=>requestResource<BroadcastPreview>(`${broadcastPath(s,id)}/preview`,'POST',{}),
    capability: (s: string) => requestResource<BroadcastCapability>(scope(s, 'broadcast-capabilities')),
    list: (s: string, page = 1, status = '') => requestCollection<BroadcastTask>(`${broadcastPath(s)}?page[number]=${page}&page[size]=30${status ? `&status=${part(status)}` : ''}`),
    task: (s: string, id: string) => requestResource<BroadcastTask>(broadcastPath(s,id)),
    save: (s: string, input: BroadcastDraftInput, id?: string, key?: string) => requestResource<BroadcastTask>(broadcastPath(s,id), id ? 'PATCH' : 'POST', input, undefined, key ? {'Idempotency-Key':key} : undefined),
    targets: (s: string, id: string, page = 1) => requestCollection<BroadcastTarget>(`${broadcastPath(s,id)}/targets?page[number]=${page}&page[size]=100`),
    observations:(s:string,id:string,page=1)=>requestCollection<BroadcastObservation>(`${broadcastPath(s,id)}/observations?page[number]=${page}&page[size]=30`),
    events: (s: string, id: string, page = 1) => requestCollection<BroadcastEvent>(`${broadcastPath(s,id)}/events?page[number]=${page}&page[size]=30`),
    action: (s: string, id: string, operation: 'confirm'|'abandon'|'refresh'|'reminders'|'stop'|'reconciliations', key: string, input: BroadcastAction = {}) => requestResource<BroadcastTask>(`${broadcastPath(s,id)}/${operation}`, 'POST', input, undefined, {'Idempotency-Key': key}),
};
const officialSenders={
    list:(school:string|undefined,page=1)=>requestCollection<OfficialSender>(`${school ? scope(school,'wecom-senders') : '/api/v1/platform/wecom-senders'}?page[number]=${page}&page[size]=30`),
    qualify:(school:string,id:string,input:{enabled:boolean;revision:number})=>requestResource<{updated:boolean}>(`/api/v1/platform/wecom-distribution/workspaces/${part(school)}/wecom-senders/${part(id)}/qualification`,'POST',input),
};
const distributionPolicies={
    get:()=>requestResource<DistributionPolicies>('/api/v1/platform/distribution-policies'),
    source:(input:SourcePolicy)=>requestResource<{updated:boolean}>('/api/v1/platform/distribution-source-policies','POST',input),
};
 return {broadcast,officialSenders,distributionPolicies};
}

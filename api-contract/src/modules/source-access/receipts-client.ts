import type {Client} from '../../http/client'
import type {SourceReceiptV2} from './protocol'
export interface ReceiptRow extends Omit<SourceReceiptV2,'retryable'|'observed_sequence'> {id:string;automation_task_id?:string|null;observed_sequence:number|null;revision:number;external_content_id:string;sequence:number;summary_version:string|null;summary_revision:number;summary_total:number|null}
export interface ReceiptHistory {revision:number;status:string;stage:string;reason_code:string|null;observed_sequence:number|null;created_at:string}
export interface ReceiptCheck {rule_revision:number;fingerprint:string;outcome:'passed'|'blocked';created_at:string}
export interface ReceiptDetail extends ReceiptRow {history:ReceiptHistory[];checks:ReceiptCheck[];relations:{related_receipt_id:string;kind:'duplicate'}[]}
const routePath=(route:string)=>`/api/v2/admin/source-routes/${encodeURIComponent(route)}/receipts`
export function createSourceReceiptsClient({requestResource}:Pick<Client,'requestResource'>){return {
 list:(route:string,page=1)=>requestResource<{items:ReceiptRow[];has_more:boolean}>(`${routePath(route)}?page=${page}`),
 detail:(route:string,receipt:string)=>requestResource<ReceiptDetail>(`${routePath(route)}/${encodeURIComponent(receipt)}`),

}}
export const receiptStatusLabels:Record<string,string>={received:'处理中',applied:'已处理',rejected:'已拒绝'}
export const receiptStageLabels:Record<string,string>={received:'已收件',delegation_pending:'处理中',task_created:'通道已受理',not_dispatched:'未下发成功'}
export const receiptReasonLabels:Record<string,string>={route_no_targets:'该消息没有目标群，未下发成功',channel_resources_missing:'没有可用发送资源，未下发成功',keyword_blocked:'消息包含敏感词，未下发成功',channel_content_invalid:'所选通道无法接受该正文',temporarily_unavailable:'处理暂时不可用，系统将自动重试'}

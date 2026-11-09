import type {Client} from '../../http/client'
import type {SourceReceiptV2} from './protocol'
export interface ReceiptRow extends Omit<SourceReceiptV2,'retryable'|'observed_sequence'> {id:string;observed_sequence:number|null;revision:number;external_content_id:string;sequence:number;summary_version:string|null;summary_revision:number;summary_total:number|null}
export interface ReceiptHistory {revision:number;status:string;stage:string;reason_code:string|null;observed_sequence:number|null;created_at:string}
export interface ReceiptCheck {rule_revision:number;fingerprint:string;outcome:'passed'|'blocked';created_at:string}
export interface ReceiptDetail extends ReceiptRow {history:ReceiptHistory[];checks:ReceiptCheck[];relations:{related_receipt_id:string;kind:'duplicate'|'update'|'withdrawal'}[];can_recheck:boolean}
const routePath=(route:string)=>`/api/v2/admin/source-routes/${encodeURIComponent(route)}/receipts`
export function createSourceReceiptsClient({requestResource}:Pick<Client,'requestResource'>){return {
 list:(route:string,page=1)=>requestResource<{items:ReceiptRow[];has_more:boolean;can_recheck:boolean}>(`${routePath(route)}?page=${page}`),
 detail:(route:string,receipt:string)=>requestResource<ReceiptDetail>(`${routePath(route)}/${encodeURIComponent(receipt)}`),
 recheck:(route:string,receipt:string,revision:number,reason:string)=>requestResource<{id:string;queued:boolean}>(`${routePath(route)}/${encodeURIComponent(receipt)}/rechecks`,'POST',{revision,reason}),
}}
export const receiptStatusLabels:Record<string,string>={received:'处理中',applied:'素材已应用',ignored_stale:'已忽略旧版本',rejected:'素材已拒绝'}
export const receiptStageLabels:Record<string,string>={received:'已收件',filter_pending:'等待检查',processing_retry:'处理重试中',recheck_pending:'等待重检',projection_applied:'素材检查通过',configuration_wait:'等待执行配置',task_created:'任务已创建',routed:'任务已创建',no_resend:'已更新素材，不重复发送',withdrawn:'已撤回',ignored_stale:'旧版本已忽略',projection_duplicate:'等价重复版本',rejected:'素材已拒绝',execution_stopped:'执行已停止',execution_blocked:'关键字准入拦截'}
export const receiptReasonLabels:Record<string,string>={ad_unavailable:'所选广告不可用，等待修复配置',ad_final_keyword_blocked:'广告组合命中最终关键字规则',ad_frozen_unavailable:'冻结广告失效，未执行目标已拦截',ad_configuration_unavailable:'广告配置暂时不可用',ad_selection_stale:'广告版本变化，等待重新选择',workspace_keyword_blocked:'原文命中工作空间规则',final_keyword_blocked:'最终正文命中拦截规则',content_check_pending:'等待当前规则检查',keyword_blocked:'原文命中拦截规则',content_version_conflict:'同序号内容冲突',content_version_superseded:'已被更高版本替代',channel_content_invalid:'所选通道无法接受该正文',content_updated_no_resend:'内容更新，未再次群发',temporarily_unavailable:'处理暂时不可用，系统将自动重试'}

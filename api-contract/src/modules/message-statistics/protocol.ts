export type Count = string
export type Outcome = 'sent'|'failed'|'unknown'|'pending'
export interface StatisticsFilter {
 start?:string; end?:string; grain?:'hour'|'day'|'week'|'month'|'year';
 platform_id?:string; business_unit_id?:string; route_id?:string;
 source_kind?:'all'|'source_route'|'manual';channel?:'wecom'|'mobile';client_kind?:'wecom'|'wechat'|'unclassified'
}
export interface NormalizedStatisticsFilter {start:string;end:string;grain:NonNullable<StatisticsFilter['grain']>;platform_id:string|null;business_unit_id:string|null;route_id:string|null;source_kind:NonNullable<StatisticsFilter['source_kind']>;channel:'wecom'|'mobile'|null;client_kind:'wecom'|'wechat'|'unclassified'|null}
export interface Counts {total:Count;sent:Count;failed:Count;unknown:Count;pending:Count}
export interface Card {key:'day'|'week'|'month'|'year';start:string;end:string;comparison_start:string;comparison_end:string;count:Count;previous:Count}
export interface Bucket {start:string;received:Count;targets:Counts}
export interface Breakdown {id:string;name:string;platform_name:string|null;received:Count;targets:Counts;responsibilities:Counts}
export interface Failure {object:'message'|'target';reason_code:string;count:Count}
export interface StatisticsOverview {
 id:string;metric_version:string;generated_at:string;expires_at:string;timezone:'Asia/Shanghai';basis:'cohort';filter:NormalizedStatisticsFilter;
 summary:{received:Count;dispatched:Count;not_dispatched:Count;partial_channel_failure:Count;processing:Count;incomplete:Count;historical_risk:Count;targets:Counts;responsibilities:Counts;attention:Counts};
 cards:Card[];trend:Bucket[];channels:Breakdown[];units:Breakdown[];failures:Failure[];unclassified:Count
}
export interface DetailFilter {bucket?:string;issues?:boolean;object?:'message'|'target'|'responsibility';outcome?:Outcome|'not_dispatched';reason_code?:string;unit?:string;attention?:boolean;page?:number}
export interface Detail {id:string;receipt_id:string|null;task_id:string|null;target_name:string|null;unit_name:string;channel:'wecom'|'mobile'|null;client_kind:'wecom'|'wechat'|'unclassified'|null;cohort_at:string;outcome:string;reason_code:string|null;device_id:string|null;historical_risk:boolean}
export interface DetailPage {items:Detail[];has_more:boolean;snapshot_id:string}
export interface StatisticsExport {filename:string;content:string;row_count:Count}

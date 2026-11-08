/** Published server-to-server wire types. Signing and credentials stay in Rust. */
export interface SourceEventV2 {event_id:string;schema_version:2;route_id:string;event_type:'content.upsert'|'content.withdrawn';external_content_id:string;version:string;sequence:number;occurred_at:string;title:string|null;content:string|null;detail_url:string}
export interface SourceReceiptV2 {event_id:string;status:'received'|'applied'|'ignored_stale'|'rejected';reason_code:string|null;processing_stage:string;received_at:string;updated_at:string;retryable:boolean;sending:{pending:number;sent:number;failed:number;unknown:number}|null}
export interface SourceCheckV2 {route_id?:string|null;cursor?:string|null;limit?:number|null}
export interface SourceCapabilitiesV2 {source_id:string;signature_versions:number[];schema_versions:number[];credential_revision:number;authorization_revision:number;paths:{authentication:string;events:string;receipts:string;routes:string};routes:{route_id:string;available:boolean}[];next_cursor:string|null}
export interface SourceDocument<T> {data:{type:string;id:string;attributes:T}}

/** Published server-to-server wire types. Signing and credentials stay in Rust. */
export interface SourceEventV2 {event_id:string;schema_version:2;route_id:string;event_type:'content.upsert';occurred_at:string;content:string}
export interface SourceReceiptV2 {event_id:string;status:'received';received_at:string}
export interface SourceCheckV2 {route_id?:string|null;cursor?:string|null;limit?:number|null}
export interface SourceCapabilitiesV2 {source_id:string;signature_versions:number[];schema_versions:number[];credential_revision:number;authorization_revision:number;paths:{authentication:string;events:string;receipts:string;routes:string};routes:{route_id:string;available:boolean;display_name?:string}[];next_cursor:string|null}
export interface SourceDocument<T> {data:{type:string;id:string;attributes:T}}

import {requestCollection,type Cancellation} from '../../http'
export interface AuditEvent {
    id: string;
    actor_id: string | null;
    action: string;
    object_id: string | null;
    request_id: string | null;
    detail: unknown;
    created_at: string;
}
export const auditEvents = {
    list: (query = '', signal?: Cancellation) => requestCollection<AuditEvent>('/api/v1/audit-events' + query, signal),
};

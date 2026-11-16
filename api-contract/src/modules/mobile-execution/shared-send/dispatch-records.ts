import { requestResource } from "../../../http";
import type { Client } from "../../../http/client";
export interface DispatchQuery {
  cursor?: string;
  page_size?: number;
  source_kind?: string;
  message_id?: string;
  receipt_id?: string;
  task_id?: string;
  target?: string;
  device_id?: string;
  stage?: string;
  reason?: string;
  reason_scope?: string;
  outcome?: string;
  from?: string;
  until?: string;
  kind?: string;
  result?: string;
  round_no?: number;
  configuration_id?: string;
  intake_id?: string;
  execution_id?: string;
}
export interface DispatchPage<T> {
  items: T[];
  has_more: boolean;
  next_cursor: string | null;
  observed_at: string;
  version: string;
}
export interface DispatchIntake {
  candidate_reasons?: Array<{
    device_id: string | null;
    device_name: string | null;
    reason_code: string;
    reason_label: string;
    checked_at: string;
  }>;
  id: string;
  source_kind: string;
  message_id: string | null;
  receipt_id: string | null;
  configuration_id: string | null;
  target_id: string | null;
  target_name: string;
  task_id: string | null;
  item_id: string | null;
  created_at: string;
  creation_state: string;
  content_summary: string;
  content?: string;
  device_id: string | null;
  device_name: string | null;
  attempt_id: string | null;
  attempt_no: number | null;
  execution_id: string | null;
  generated_at: string | null;
  received_at: string | null;
  result_deadline_at: string | null;
  round_deadline_at: string | null;
  completed_rounds: number | null;
  max_rounds: number | null;
  next_evaluation_at: string | null;
  outcome: string;
  stage: string;
  reason_code: string;
  reason_label: string;
  last_progress_at: string;
  last_checked_at: string | null;
  historical_risk: boolean;
  version: string;
  owner: string;
  next_action: string;
  observed_at?: string;
}
export interface DispatchOperation {
  id: string;
  intake_id: string;
  kind: string;
  round_no: number | null;
  attempt_id: string | null;
  result: string;
  reason_code: string | null;
  reason_label: string;
  started_at: string;
  finished_at: string | null;
  retry_at: string | null;
}
export interface DispatchCheck {
  id: string;
  operation_id: string | null;
  delivery_request_id: string | null;
  intake_id: string | null;
  device_id: string | null;
  check_kind: string;
  decision: string;
  reason_code: string;
  reason_label: string;
  evidence: Record<string, unknown>;
  created_at: string;
}
export interface DispatchEvent {
  id: number;
  intake_id: string | null;
  operation_id: string | null;
  delivery_request_id: string | null;
  item_id: string | null;
  round_no: number | null;
  kind: string;
  reason: string;
  result: string;
  device_id: string | null;
  attempt_id: string | null;
  execution_id: string | null;
  evidence: Record<string, unknown>;
  created_at: string;
  reason_label?: string;
}
export interface DeliveryExchange {
  id: string;
  device_id: string;
  generation: number;
  key_digest: string;
  replay_of: string | null;
  intake_id: string | null;
  attempt_id: string | null;
  execution_id: string | null;
  result: string;
  reason_code: string | null;
  reason_label: string;
  response_state: string;
  created_at: string;
  finished_at: string | null;
  response_at: string | null;
}
const root = "/api/v2/admin/mobile-dispatch";
export function createDispatchRecordsClient({
  requestResource,
}: Pick<Client, "requestResource">) {
  const query = (q: DispatchQuery) =>
    Object.entries(q)
      .filter(([, v]) => v !== undefined && v !== "")
      .map(
        ([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`,
      )
      .join("&");
  const list = <T>(path: string, q: DispatchQuery = {}) =>
    requestResource<DispatchPage<T>>(`${root}/${path}?${query(q)}`);
  return {
    list: (q: DispatchQuery = {}) => list<DispatchIntake>("intakes", q),
    detail: (id: string) =>
      requestResource<DispatchIntake>(
        `${root}/intakes/${encodeURIComponent(id)}`,
      ),
    operations: (id: string, q: DispatchQuery = {}) =>
      list<DispatchOperation>(
        `intakes/${encodeURIComponent(id)}/operations`,
        q,
      ),
    events: (id: string, q: DispatchQuery = {}) =>
      list<DispatchEvent>(`intakes/${encodeURIComponent(id)}/events`, q),
    checks: (id: string, q: DispatchQuery = {}) =>
      list<DispatchCheck>(`operations/${encodeURIComponent(id)}/checks`, q),
    exchanges: (q: DispatchQuery = {}) =>
      list<DeliveryExchange>("delivery-exchanges", q),
    exchange: (id: string) =>
      requestResource<DeliveryExchange>(
        `${root}/delivery-exchanges/${encodeURIComponent(id)}`,
      ),
    exchangeChecks: (id: string, q: DispatchQuery = {}) =>
      list<DispatchCheck>(
        `delivery-exchanges/${encodeURIComponent(id)}/checks`,
        q,
      ),
  };
}
export const dispatchRecords = createDispatchRecordsClient({ requestResource });

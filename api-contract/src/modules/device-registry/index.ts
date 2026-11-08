import {requestResource,requestCollection,type Collection} from "../../http";
export type UUID=string;
export interface Row {
  id: UUID;
  revision: number;
}
export type DeviceControlKind = "task_available" | "configuration_changed" | "pause_execution" | "resume_execution" | "request_status" | "cancel_execution";
export interface DeviceControlCommand {
  id: UUID; sequence: number; kind: DeviceControlKind;
  state: "queued" | "delivered" | "accepted" | "completed" | "rejected" | "expired";
  expires_at: string; result_code: string | null;
}
export interface DeviceControlSnapshot {
  id: UUID; state: "connecting" | "online" | "suspect" | "offline" | "disabled" | "unauthorized";
  last_control_heartbeat_at: string | null; transport: "wss" | "https" | null;
  readiness: string | null; block_reason: string | null;
  desired_paused: boolean; desired_revision: number; credential_expires_at: string | null;
  transport_connected?: boolean;
  server_time?: string;
  heartbeat_interval_seconds?: number; suspect_after_seconds?: number; offline_after_seconds?: number;
  connected_at?: string | null; last_disconnected_at?: string | null; disconnect_reason?: string | null;
}
export interface DeviceControlPresence extends DeviceControlSnapshot {
  commands: DeviceControlCommand[];
}
export interface DeviceControlPulse {
  connection_id: UUID; epoch: number; heartbeat_id: UUID; heartbeat_sequence: number; probe_id?: UUID | null;
  readiness: string; block_reason: string | null;
  receipts: Array<{ id: UUID; state: "accepted" | "completed" | "rejected"; result_code: string | null }>;
}
export interface DeviceControlFrame {
  id: UUID; connection_id: UUID; epoch: number; protocol_version: 1;
  probe_id?: UUID; heartbeat_id: UUID | null; heartbeat_interval_seconds: number; offline_after_seconds: number;
  server_time: string; desired_paused: boolean; desired_revision: number;
  credential_expires_at: string; commands: Array<Omit<DeviceControlCommand, "result_code"> & { resource_id: UUID | null; desired_revision: number | null }>;
}
export interface Device extends Row {
  control?: DeviceControlSnapshot;
  execution_protocol_version?: 10;
  supported_protocol_versions?: number[];
  name: string;
  enabled: boolean;
  credential_generation: number;
  client_instance_id: string | null;
  last_seen_at: string | null;
  created_at: string;
}
export interface Enrollment {
  id: UUID;
  device_id: UUID;
  code?: string;
  expires_at: string;
  secret_replay_unavailable?: boolean;
}
export interface RevisionReason {
  expected_revision: number;
  reason: string;
}
const segment = (id: string) => encodeURIComponent(id);
const devices = "/api/v1/platform/execution-devices";
function write<T>(
  path: string,
  method: string,
  input: unknown,
  key: string,
): Promise<T> {
  if (!key.trim() || key.length > 128)
    throw new Error("Idempotency-Key must contain 1–128 characters");
  return requestResource<T>(path, method, input, undefined, {
    "Idempotency-Key": key,
  });
}
function list<T>(path: string, page = 1, size = 30): Promise<Collection<T>> {
  return requestCollection<T>(
    `${path}?page[number]=${page}&page[size]=${size}`,
  );
}
export const deviceRegistryApi={
  devices: (page?: number, size?: number) => list<Device>(devices, page, size),
  device: (id: UUID) => requestResource<Device>(`${devices}/${segment(id)}`),
  /** New registrations are enabled; existing disabled/revoked devices are not re-enabled. */
  createDevice: (input: { name: string }, key: string) =>
    write<Device>(devices, "POST", input, key),
  updateDevice: (
    id: UUID,
    input: RevisionReason & { enabled: boolean },
    key: string,
  ) => write<Device>(`${devices}/${segment(id)}`, "PATCH", input, key),
  enrollment: (id: UUID, input: { expected_revision: number }, key: string) =>
    write<Enrollment>(
      `${devices}/${segment(id)}/enrollments`,
      "POST",
      input,
      key,
    ),
  revoke: (id: UUID, input: RevisionReason, key: string) =>
    write<Device>(
      `${devices}/${segment(id)}/credential-revocations`,
      "POST",
      input,
      key,
    ),
  controlPresence: (id: UUID) => requestResource<DeviceControlPresence>(`${devices}/${segment(id)}/control`),
  controlCommand: (id: UUID, kind: DeviceControlKind, key: string) => write<DeviceControlCommand>(`${devices}/${segment(id)}/control`, "POST", { kind }, key),
}

import { requestResource, requestCollection } from "../../http";
import type { UUID, Row, MobileItem } from "./index";
import type {
  ConfigurationInput,
  UiRules,
  ManualBaseline,
  RunProgress,
} from "./configuration";
import type { LauncherFlow } from "./automation";
export type FlowClient = { package_name: string };
export type FlowRules = Omit<UiRules, "client"> & { client: FlowClient };
export type FlowConfiguration = Omit<ConfigurationInput, "rules"> & {
  rules: FlowRules;
};
export type FlowLauncher = Omit<LauncherFlow, "client"> & {
  client: FlowClient;
};
export interface FlowSpecialization {
  key: string;
  minimum_version_code: number;
  maximum_version_code: number;
  rules: FlowRules;
}
export interface DispatchFlowV1 {
  schema_version: 1;
  app_key: "wecom";
  capability: "group_text";
  application_label: string;
  configuration: FlowConfiguration;
  launchers: FlowLauncher[];
  specializations: FlowSpecialization[];
  max_attempts: number;
  max_total_steps: number;
  max_total_scrolls: number;
  max_total_duration_ms: number;
  recoverable_errors: string[];
}
export interface PacingProfile extends Pick<Row, "id"> {
  name: string;
  school_id: UUID;
  validation_ids: UUID[];
  baseline: ManualBaseline;
}
export interface FlowValidation extends Row {
  school_id: UUID;
  device_id: UUID;
  flow_id: UUID | null;
  kind: "measure_manual" | "open_application" | "inspect_roles";
  path_key: string;
  launcher_index: number | null;
  state: string;
  generation: UUID;
  expires_at: string;
  result: unknown;
}
export interface FlowVersion extends Pick<Row, "id"> {
  name: string;
  body: DispatchFlowV1;
  content_digest: string;
  pacing_profile_id: UUID;
}
export interface FlowRelease extends Row {
  flow_id: UUID;
  school_id: UUID;
  validation_id: UUID | null;
  flow_validation_id: UUID | null;
  active: boolean;
  revoked: boolean;
  flow: DispatchFlowV1;
  name: string;
  flow_digest: string;
}
export interface ExecutionAssignment extends Row {
  school_id: UUID;
  device_id: UUID;
  sender_account_id: UUID;
  account_binding_revision: number;
  release_id: UUID;
  enabled: boolean;
  device_name: string;
  account_name: string;
  flow_name: string;
  flow: DispatchFlowV1;
  online: boolean;
  protocol: number;
}
export interface AutomationDeviceGroup extends Row {
  school_id: UUID;
  name: string;
  device_ids: UUID[];
}
export interface DispatchInput {
  name: string;
  text_content: string;
  interval_seconds: number;
  targets: Array<{ assignment_id: UUID; group_ids: UUID[] }>;
}
export interface DispatchPlanBody {
  schema_version: 1;
  protocol_version: 5;
  batch_id: UUID;
  run_id: UUID;
  school_id: UUID;
  device_id: UUID;
  sender_account_id: UUID;
  binding_revision: number;
  assignment_id: UUID;
  assignment_revision: number;
  release_id: UUID;
  flow: DispatchFlowV1;
  flow_digest: string;
  sender: Record<string, unknown>;
  groups: Array<Record<string, unknown>>;
  group_ids: UUID[];
  text_content: string;
  interval_ms: number;
}
export interface DispatchPlan extends Pick<Row, "id"> {
  batch_id: UUID;
  run_id: UUID;
  body: DispatchPlanBody;
  content_digest: string;
  device_name?: string;
  delivery?: TaskDelivery;
  progress?: RunProgress;
  items?: Array<MobileItem & { position: number }>;
}
export interface DispatchPreviewTarget {
  assignment: ExecutionAssignment;
  release: FlowRelease;
  groups: Array<{
    group: { id: UUID; name: string };
    sender: { id: UUID };
    content: string;
  }>;
}
export interface DispatchBatch extends Row {
  school_id: UUID;
  name: string;
  state: "draft" | "confirmed" | "paused" | "cancelled";
  input: DispatchInput;
  preview: { input: DispatchInput; targets: DispatchPreviewTarget[] };
  preview_digest: string;
  plans?: DispatchPlan[];
  confirmed_at: string | null;
}
export interface TaskDelivery extends Pick<Row, "id"> {
  plan_id: UUID;
  device_id: UUID;
  received_at: string | null;
  generation: UUID;
  cancelled: boolean;
  plan: DispatchPlan;
  batch_state: DispatchBatch["state"];
  progress: RunProgress;
}
export interface FailureVersions {
  application_name: string | null;
  application_code: number | null;
  engine: string | null;
  android: string | null;
  launcher: string | null;
}
export interface DispatchFailure {
  attempted_paths?: string[];
  budget?: {
    attempts: number;
    steps: number;
    scrolls: number;
    elapsed_ms: number;
  } | null;
  event_id: UUID;
  item_id: UUID;
  fence: number;
  plan_digest: string;
  path_key: string;
  step: string;
  reason: string;
  send_state: "not_sent" | "unknown";
  versions: FailureVersions | null;
}
export interface ExecutionFailure extends Pick<Row, "id"> {
  occurrences: number;
  delivery_id: UUID;
  school_id: UUID;
  event_id: UUID;
  body: DispatchFailure;
  fingerprint: string;
  created_at: string;
}
export interface BusinessApplicationScope {
  format: "business_application_scope/2";
  origin: string;
  device_id: UUID;
  sender_account_id: UUID;
  schools: UUID[];
  app_key: "wecom";
  capability: "group_text";
}
export interface BusinessAuthorizationReceipt {
  authorization_id: UUID;
  revision: number;
  state: "effective" | "revoked";
  granted_at: string;
  revoked_at: string | null;
  scope: BusinessApplicationScope;
  binding_revision?: number;
}
export interface DispatchPreparation extends Row {
  delivery_id: UUID;
  item_id: UUID;
  credential_generation: number;
  generation: UUID;
  authorization_id: UUID;
  fence: number;
  stage: "preparing_app" | "ready";
  path_key: string;
  path_digest: string | null;
  ready: boolean;
  expires_at: string;
  plan?: DispatchPlan;
}
const id = encodeURIComponent;
const school = (s: UUID) => `/api/v1/schools/${id(s)}`;
const platform = "/api/v1/platform";
const list = <T>(path: string, page = 1) =>
  requestCollection<T>(`${path}?page[number]=${page}&page[size]=100`);
const write = <T>(path: string, input: unknown, key: string) => {
  if (!key.trim() || key.length > 128)
    throw new Error("Invalid Idempotency-Key");
  return requestResource<T>(path, "POST", input, undefined, {
    "Idempotency-Key": key,
  });
};
export interface SchoolExecutionResource {
  id: UUID; device_id: UUID; device_name: string; sender_account_id: UUID;
  account_name: string; company_name: string;
  control_state: import('./index').DeviceControlSnapshot['state'];
  last_control_heartbeat_at: string | null; available: boolean; block_reason: string | null;
}
export const dispatchApi = {
  resources: (s: UUID, page = 1) => list<SchoolExecutionResource>(`${school(s)}/execution-resources`, page),
  validations: (s: UUID, page = 1) =>
    list<FlowValidation>(`${school(s)}/automation-flow-validations`, page),
  validate: (
    s: UUID,
    input: {
      device_id: UUID;
      flow_id: UUID | null;
      manual_application: { label: string; client: FlowClient } | null;
      kind: FlowValidation["kind"];
      path_key: string;
      launcher_index: number | null;
    },
    key: string,
  ) =>
    write<FlowValidation>(
      `${school(s)}/automation-flow-validations`,
      input,
      key,
    ),
  cancelValidation: (s: UUID, j: FlowValidation, key: string) =>
    write<FlowValidation>(
      `${school(s)}/automation-flow-validations/${id(j.id)}/cancellations`,
      { expected_revision: j.revision },
      key,
    ),
  flows: (page = 1) => list<FlowVersion>(`${platform}/automation-flows`, page),
  createFlow: (flow: DispatchFlowV1, pacing_profile_id: UUID, key: string) =>
    write<FlowVersion>(
      `${platform}/automation-flows`,
      { flow, pacing_profile_id },
      key,
    ),
  pacing: (page = 1) =>
    list<PacingProfile>(`${platform}/automation-pacing-profiles`, page),
  createPacing: (
    name: string,
    school_id: UUID,
    validation_ids: UUID[],
    key: string,
  ) =>
    write<PacingProfile>(
      `${platform}/automation-pacing-profiles`,
      { name, school_id, validation_ids },
      key,
    ),
  publish: (flow: UUID, school_id: UUID, validation_id: UUID, key: string) =>
    write<FlowRelease>(
      `${platform}/automation-flows/${id(flow)}/releases`,
      { school_id, validation_id },
      key,
    ),
  releases: (s?: UUID, page = 1) =>
    list<FlowRelease>(
      `${s ? school(s) : platform}/automation-flow-releases`,
      page,
    ),
  revoke: (release: FlowRelease, reason: string, key: string) =>
    write<FlowRelease>(
      `${platform}/automation-flow-releases/${id(release.id)}/revocations`,
      { expected_revision: release.revision, reason },
      key,
    ),
  groups: (s: UUID, page = 1) =>
    list<AutomationDeviceGroup>(`${school(s)}/automation-device-groups`, page),
  saveGroup: (s: UUID, name: string, device_ids: UUID[], key: string) =>
    write<AutomationDeviceGroup>(
      `${school(s)}/automation-device-groups`,
      { name, device_ids },
      key,
    ),
  assignments: (s: UUID, page = 1) =>
    list<ExecutionAssignment>(
      `${school(s)}/automation-execution-assignments`,
      page,
    ),
  assign: (
    s: UUID,
    device_ids: UUID[],
    group_id: UUID | null,
    release_id: UUID,
    key: string,
  ) =>
    write<{ id: UUID; assignments: ExecutionAssignment[] }>(
      `${school(s)}/automation-execution-assignments`,
      { device_ids, group_id, release_id },
      key,
    ),
  enabled: (s: UUID, enabled: boolean, key: string) =>
    write<Row>(
      `${school(s)}/automation-dispatch-settings`,
      { enabled, reviewed_history: enabled },
      key,
    ),
  preview: (s: UUID, input: DispatchInput, key: string) =>
    write<DispatchBatch>(
      `${school(s)}/automation-dispatch-previews`,
      { input },
      key,
    ),
  confirm: (s: UUID, batch: DispatchBatch, key: string) =>
    write<DispatchBatch>(
      `${school(s)}/automation-dispatch-previews/${id(batch.id)}/confirmations`,
      { preview_digest: batch.preview_digest, reviewed: true },
      key,
    ),
  batches: (s: UUID, page = 1) =>
    list<DispatchBatch>(`${school(s)}/automation-dispatch-batches`, page),
  batch: (s: UUID, batch: UUID) =>
    requestResource<DispatchBatch>(
      `${school(s)}/automation-dispatch-batches/${id(batch)}`,
    ),
  control: (
    s: UUID,
    b: DispatchBatch,
    command: "pause" | "resume" | "stop",
    key: string,
  ) =>
    write<DispatchBatch>(
      `${school(s)}/automation-dispatch-batches/${id(b.id)}/controls`,
      { expected_revision: b.revision, command },
      key,
    ),
  remaining: (s: UUID, b: DispatchBatch, release_id: UUID, key: string) =>
    write<DispatchBatch>(
      `${school(s)}/automation-dispatch-batches/${id(b.id)}/remaining-previews`,
      { expected_revision: b.revision, release_id, reviewed: true },
      key,
    ),
  failures: (s?: UUID, page = 1) =>
    list<ExecutionFailure>(
      `${s ? school(s) : platform}/automation-execution-failures`,
      page,
    ),
};

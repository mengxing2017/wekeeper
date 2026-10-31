import { requestResource, requestCollection } from "../../http";
import type { UUID, Row, StrategyRun, Preview } from "./index";
import type {
  UiClient,
  UiRules,
  UiSelector,
  ExecutionPolicy,
  RunProgress,
  ConfigurationInput,
} from "./configuration";
export interface AutomationApp {
  id: UUID;
  app_key: string;
  name: string;
  enabled: boolean;
  capabilities: Array<{
    id: UUID;
    name: string;
    capability_key: string;
    minimum_protocol: 4;
  }>;
}
export interface InstalledApplication {
  app_key: string;
  instance_ref: string;
  label: string;
  client: UiClient;
  instance_visibility: "unique" | "ambiguous" | "unavailable";
}
export interface EnvironmentReport {
  generation: number;
  launcher: UiClient;
  applications: InstalledApplication[];
  engine_version: string;
  capabilities: string[];
  notifications_ready: boolean;
  accessibility_ready: boolean;
  unlocked: boolean;
  maintenance_mode: boolean;
}
export interface DeviceEnvironment {
  id: UUID;
  device_id: UUID;
  generation: number;
  body: EnvironmentReport;
  content_digest: string;
  received_at: string;
}
export interface DeviceApplications {
  id: UUID;
  environment: DeviceEnvironment | null;
  status: "reported" | "waiting_environment";
  next_step: string;
}
export interface AppBinding extends Row {
  school_id: UUID;
  device_id: UUID;
  app_id: UUID;
  sender_account_id: UUID;
  account_binding_revision: number;
  instance_ref: string;
  environment_id: UUID;
  device_name?: string;
  app_name?: string;
  account_name?: string;
}
export type ValidationKind =
  | "inspect_environment"
  | "open_application"
  | "inspect_roles"
  | "measure_manual";
export type ValidationState =
  | "queued"
  | "offered"
  | "accepted"
  | "running"
  | "succeeded"
  | "failed"
  | "interrupted"
  | "cancelled"
  | "expired"
  | "declined";
export interface ManualTiming {
  group_interval_ms: number;
  navigation_ms: number;
  input_codepoint_ms: number;
  input_settle_ms: number;
  observation_ms: number;
}
export interface ValidationEvent {
  event_id: UUID;
  generation: UUID;
  expected_revision: number;
  stage: string;
  outcome: "running" | "succeeded" | "failed" | "interrupted";
  role_matches: Record<string, number>;
  timing: ManualTiming | null;
  path: "search" | "all_apps" | null;
}
export interface ValidationJob extends Row {
  authorization_id?: UUID | null;
  decision_source?: "local_action" | "standing_authorization" | "local_decline" | null;
  school_id: UUID;
  device_id: UUID;
  app_binding_id: UUID | null;
  binding_revision: number | null;
  kind: ValidationKind;
  state: ValidationState;
  generation: UUID;
  component_ids: UUID[];
  environment_id: UUID | null;
  expires_at: string;
  result: ValidationEvent | null;
  components?: AutomationComponent[];
  application?: InstalledApplication;
  launcher?: UiClient;
}
export type LauncherOperation =
  | { action: "home" | "verify_application" }
  | { action: "click" | "search_application" | "scroll_forward"; role: string };
export interface LauncherStep {
  from: string;
  to: string;
  operation: LauncherOperation;
  delay_ms: number;
}
export interface LauncherFlow {
  client: UiClient;
  pages: UiRules["pages"];
  roles: Record<string, UiSelector>;
  search_steps: LauncherStep[];
  all_apps_steps: LauncherStep[];
  max_steps: number;
  timeout_ms: number;
  max_scrolls: number;
  local_application_results_only: true;
}
export type AutomationComponentBody =
  | { kind: "launcher"; flow: LauncherFlow }
  | { kind: "workflow"; rules: UiRules }
  | { kind: "policy"; policy: ExecutionPolicy }
  | { kind: "baseline"; validation_ids: UUID[] };
export interface AutomationComponent extends Row {
  name: string;
  kind: AutomationComponentBody["kind"];
  body: AutomationComponentBody;
  content_digest: string;
}
export interface AutomationRelease extends Row {
  component_id: UUID;
  kind: AutomationComponentBody["kind"];
  school_id: UUID;
  device_id: UUID | null;
  environment_digest: string;
  active: boolean;
  revoked: boolean;
  validation_id: UUID;
}
export interface AutomationBusinessInput {
  app_binding_id: UUID;
  capability_id: UUID;
  name: string;
  group_ids: UUID[];
  text_content: string;
  interval_seconds: number;
  schedule: "manual_once";
}
export interface CapabilityResolution {
  binding: AppBinding;
  environment_id: UUID;
  environment_generation: number;
  environment_digest: string;
  configuration: ConfigurationInput;
  components: Record<
    string,
    AutomationRelease & { component: AutomationComponent }
  >;
  protocol_version: 4;
  engine_version: string;
}
export interface AutomationPreview extends Row {
  input: AutomationBusinessInput;
  resolution: CapabilityResolution;
  preview_digest: string;
  run_id: UUID;
  task_preview: Preview;
  expires_at: string;
}
export interface AutomationRun extends StrategyRun {
  name: string;
  automation_manifest_id: UUID;
}
const school = (s: UUID) => `/api/v1/schools/${encodeURIComponent(s)}`;
const platform = "/api/v1/platform/automation-adaptations";
const id = encodeURIComponent;
const list = <T>(path: string, page = 1) =>
  requestCollection<T>(`${path}?page[number]=${page}&page[size]=100`);
function write<T>(path: string, input: unknown, key: string): Promise<T> {
  if (!key.trim() || key.length > 128)
    throw new Error("Idempotency-Key must contain 1–128 characters");
  return requestResource<T>(path, "POST", input, undefined, {
    "Idempotency-Key": key,
  });
}
export const automationApi = {
  schools: () =>
    requestResource<Array<{ id: UUID; name: string }>>(
      "/api/v1/school-directory",
    ),
  resolution: (s: UUID, binding: UUID) =>
    requestResource<CapabilityResolution>(
      `${school(s)}/device-app-bindings/${id(binding)}/capability-resolution`,
    ),
  apps: (s: UUID) => list<AutomationApp>(`${school(s)}/automation-apps`),
  applications: (s: UUID, d: UUID) =>
    requestResource<DeviceApplications>(
      `${school(s)}/automation-devices/${id(d)}/applications`,
    ),
  bindings: (s: UUID, page = 1) =>
    list<AppBinding>(`${school(s)}/device-app-bindings`, page),
  bind: (
    s: UUID,
    input: {
      device_id: UUID;
      app_id: UUID;
      environment_id: UUID;
      instance_ref: string;
      expected_revision: number;
    },
    key: string,
  ) => write<AppBinding>(`${school(s)}/device-app-bindings`, input, key),
  validations: (s: UUID, page = 1) =>
    list<ValidationJob>(`${school(s)}/automation-validations`, page),
  validation: (s: UUID, job: UUID) =>
    requestResource<ValidationJob>(
      `${school(s)}/automation-validations/${id(job)}`,
    ),
  validate: (
    s: UUID,
    input: {
      device_id: UUID;
      app_binding_id: UUID | null;
      kind: ValidationKind;
      component_ids: UUID[];
    },
    key: string,
  ) => write<ValidationJob>(`${school(s)}/automation-validations`, input, key),
  cancel: (s: UUID, job: UUID, revision: number, key: string) =>
    write<ValidationJob>(
      `${school(s)}/automation-validations/${id(job)}/cancellations`,
      { expected_revision: revision },
      key,
    ),
  preview: (s: UUID, input: AutomationBusinessInput, key: string) =>
    write<AutomationPreview>(
      `${school(s)}/automation-business-previews`,
      { input },
      key,
    ),
  confirm: (s: UUID, preview: AutomationPreview, key: string) =>
    write<AutomationRun>(
      `${school(s)}/automation-business-previews/${id(preview.id)}/confirmations`,
      {
        expected_revision: preview.revision,
        preview_digest: preview.preview_digest,
        reviewed: true,
      },
      key,
    ),
  runs: (s: UUID, page = 1) =>
    list<AutomationRun>(`${school(s)}/automation-runs`, page),
  run: (s: UUID, run: UUID) =>
    requestResource<
      RunProgress & {
        task_id: UUID;
        preparation?: { stage: string; expires_at: string } | null;
      }
    >(`${school(s)}/automation-runs/${id(run)}`),
  components: (page = 1) =>
    list<AutomationComponent>(`${platform}/components`, page),
  createComponent: (
    input: { name: string; body: AutomationComponentBody },
    key: string,
  ) => write<AutomationComponent>(`${platform}/components`, input, key),
  releases: (page = 1) => list<AutomationRelease>(`${platform}/releases`, page),
  publish: (
    component: AutomationComponent,
    input: { school_id: UUID; device_id: UUID | null; validation_id: UUID },
    key: string,
  ) =>
    write<AutomationRelease>(
      `${platform}/components/${id(component.id)}/releases`,
      { ...input, expected_revision: component.revision },
      key,
    ),
  revoke: (release: AutomationRelease, reason: string, key: string) =>
    write<AutomationRelease>(
      `${platform}/releases/${id(release.id)}/revocations`,
      { expected_revision: release.revision, reason },
      key,
    ),
};

export interface ExecutionAuthorizationScope {
  origin: string; device_id: UUID; sender_account_id: UUID;
  schools: Array<{school_id: UUID; applications: Array<{binding_id: UUID; app_key: "wecom"; instance_ref: string}>}>;
  purpose: "company_business/1"; data_policy: "minimal_visible_results/1"; object_scope: "approved_school_groups/1";
}
export interface ExecutionAuthorizationReceipt {
  authorization_id: UUID; revision: number; state: "effective" | "revoked";
  notice_version: "internal_business/1"; notice_digest: string; identity_assurance: "local_action";
  granted_at: string; revoked_at: string | null; maintenance: boolean; scope: ExecutionAuthorizationScope;
}
export interface ExecutionAuthorizationContext {
  id: UUID; authorization_revision: number; environment_generation: number; extension: "standing_authorization/1"; device_id: UUID; device_name: string;
  account_name: string; responsible_actor_id: UUID;
  schools: Array<{school_id: UUID; name: string; applications: Array<{binding_id: UUID; app_key: "wecom"; instance_ref: string; label: string}>}>;
  scope: Omit<ExecutionAuthorizationScope,"origin">;
}

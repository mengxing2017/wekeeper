/** HTTP protocol only. This package never clicks, injects, executes scripts or retries a click. */
import { ApiError, decodeData, type ResourceDocument } from "../../http";
import type {
  ConfigurationRelease,
  RunProgress,
  UiClient,
  MobileItem,
  UUID,
  VisibleVerification,
} from "../mobile-delivery";
export interface Credential {
  id: UUID;
  device_token?: string;
  credential_generation: number;
  expires_at: string;
  protocol_version: number;
  secret_replay_unavailable?: boolean;
}
export interface Binding {
  device_id: UUID;
  sender_account_id: UUID;
  revision: number;
  identity_evidence_ref: string;
}
export interface Session {
  id: UUID;
  credential_generation: number;
  binding: Binding | null;
  server_time: string;
  minimum_protocol_version: number;
  supported_protocol_versions?: number[];
  configuration_clients?: UiClient[];
}
export interface Wait {
  run_id?: UUID;
  position?: number;
  server_time: string;
  retry_after_seconds: number;
  next_eligible_at: string | null;
  reason: string;
}
export type ResultAcknowledgement = MobileItem & {
  run_progress?: Omit<RunProgress, "items" | "server_time">;
};
export interface ExecutionTarget {
  device_id: UUID;
  device_binding_revision: number;
  strategy_id: UUID;
  strategy_revision: number;
  run_id: UUID;
}
export interface Claim {
  configuration?: ConfigurationRelease | null;
  sequence?: {
    run_id: UUID;
    position: number;
    total: number;
    group_interval_ms: number;
  } | null;
  execution_revision?: number | null;
  execution_target?: ExecutionTarget | null;
  id: UUID;
  claim_id: UUID;
  item_id: UUID;
  school_id: UUID;
  state: "claimed" | "prepared" | "permit_issued" | "closed" | "expired";
  item_state: MobileItem["state"];
  fence: number;
  binding_revision: number;
  lease_until: string;
  group_snapshot: Record<string, unknown>;
  sender_binding: Record<string, unknown>;
  content: string;
  content_digest: string;
  snapshot_digest: string;
  requirements: {
    protocol_version: number;
    client_version: string;
    script_version: string;
    baseline_version: string;
    no_injection: true;
    no_hook: true;
    no_private_protocol: true;
    no_client_data_modification: true;
    local_invocation_consumption_required: true;
    no_external_ids?: true;
    no_resource_ids?: true;
    ordered_groups_required?: true;
    human_pacing_required?: true;
    configuration_digest?: string;
  };
  permit: {
    id: UUID;
    invocation_id: UUID;
    state: string;
    execute_before: string;
  } | null;
  server_time: string;
  may_execute?: boolean;
}
export interface Fence {
  fence: number;
}
export interface PreparationInput extends Fence {
  snapshot_digest: string;
  observed_account_id: UUID;
  observed_group_id: UUID;
  client_version: string;
  script_version: string;
  baseline_version: string;
  checks_passed: boolean;
  verification?: VisibleVerification;
}
export interface Preparation {
  id: UUID;
  claim_id: UUID;
  fence: number;
  snapshot_digest: string;
}
export interface PermitInput extends Fence {
  snapshot_digest: string;
  preparation_id: UUID;
}
export interface Permit {
  id: UUID;
  claim_id: UUID;
  invocation_id: UUID;
  fence: number;
  snapshot_digest: string;
  permit_token?: string;
  not_before: string;
  execute_before: string;
  server_time: string;
  state?: string;
  reason?: string;
  secret_replay_unavailable?: boolean;
}
/** Null click time is never success evidence; unknown keeps all existing holds. */
export interface ResultInput {
  invocation_id: UUID;
  permit_token: string;
  event_id: UUID;
  result: "ui_observed_sent" | "not_observed" | "safety_blocked";
  evidence_ref: string;
  observed_account_id: UUID;
  observed_group_id: UUID;
  content_digest: string;
  local_state: "sent_bubble_observed" | "not_observed" | "safety_blocked";
  reported_clicked_at: string | null;
  verification?: VisibleVerification;
}
export interface HeartbeatInput {
  binding_revision: number;
  sender_account_id: UUID;
  client_version: string;
  script_version: string;
  baseline_version: string;
  permissions_ready: boolean;
  current_claim_id?: UUID | null;
}
export interface DeviceTransportOptions {
  baseUrl: string;
  token?: string;
  fetch?: typeof fetch;
  protocolVersion?: 1 | 2 | 3;
}
export function createDeviceExecutionClient(options: DeviceTransportOptions) {
  const base = new URL(options.baseUrl);
  if (
    base.protocol !== "https:" ||
    base.username ||
    base.password ||
    base.search ||
    base.hash ||
    base.pathname !== "/"
  )
    throw new Error("Device baseUrl must be a fixed HTTPS origin");
  const transport = options.fetch ?? fetch;
  const protocol = options.protocolVersion ?? 1;
  if (protocol !== 1 && protocol !== 2 && protocol !== 3)
    throw new Error("Unsupported device protocol");
  async function wire<T>(
    path: string,
    type: string,
    attributes?: unknown,
    key?: string,
  ): Promise<{ value: T | null; meta?: ResourceDocument["meta"] }> {
    if (attributes !== undefined && (!key?.trim() || key.length > 128))
      throw new Error("A stable Idempotency-Key is required");
    const response = await transport(
      `${base.origin}/api/v${protocol}/device-execution/${path}`,
      {
        method: attributes === undefined ? "GET" : "POST",
        redirect: "error",
        credentials: "omit",
        cache: "no-store",
        headers: {
          Accept: "application/vnd.api+json",
          "Content-Type": "application/vnd.api+json",
          ...(options.token
            ? { Authorization: `Bearer ${options.token}` }
            : {}),
          ...(key ? { "Idempotency-Key": key } : {}),
        },
        body:
          attributes === undefined
            ? undefined
            : JSON.stringify({ data: { type, attributes } }),
      },
    );
    const doc = (await response.json()) as ResourceDocument & {
      errors?: Array<{
        code?: string;
        detail?: string;
        meta?: Record<string, unknown>;
      }>;
    };
    if (!response.ok) {
      const e = doc.errors?.[0];
      throw new ApiError(
        response.status,
        e?.code ?? "http_error",
        e?.detail ?? "Device request failed",
        e?.meta,
      );
    }
    if (
      response.headers.get("Content-Type")?.split(";")[0] !==
        "application/vnd.api+json" ||
      !("data" in doc) ||
      Array.isArray(doc.data) ||
      (doc.data !== null && doc.data.type !== type)
    )
      throw new ApiError(
        response.status,
        "invalid_document",
        "Invalid device JSON:API document",
      );
    return { value: decodeData<T | null>(doc), meta: doc.meta };
  }
  async function one<T>(
    path: string,
    type: string,
    input?: unknown,
    key?: string,
  ): Promise<T> {
    const r = await wire<T>(path, type, input, key);
    if (!r.value)
      throw new ApiError(200, "invalid_document", "Expected resource");
    return r.value;
  }
  const claimPath = (id: UUID) => `claims/${encodeURIComponent(id)}`;
  return {
    resolveConfiguration: (
      input: {
        package_name: string;
        version_name: string;
        version_code: number;
        engine_version: string;
        capabilities: string[];
      },
      key: string,
    ) =>
      one<ConfigurationRelease>(
        "configuration-resolutions",
        "configuration-releases",
        input,
        key,
      ),
    configuration: (id: UUID) =>
      one<ConfigurationRelease>(
        `configuration-releases/${encodeURIComponent(id)}`,
        "configuration-releases",
      ),
    enroll: (
      input: {
        device_id: UUID;
        code: string;
        client_instance_id: string;
        protocol_version: number;
      },
      key: string,
    ) => one<Credential>("enrollments", "device-credentials", input, key),
    rotate: (key: string) =>
      one<Credential>("credential-rotations", "device-credentials", {}, key),
    session: () => one<Session>("session", "device-execution-sessions"),
    heartbeat: (input: HeartbeatInput, key: string) =>
      one<{
        id: UUID;
        binding: Binding;
        permissions_ready_declared: boolean;
        server_time: string;
        heartbeat_interval_seconds: number;
      }>("heartbeats", "device-heartbeats", input, key),
    claim: async (
      input: { binding_revision: number },
      key: string,
    ): Promise<{ claim: Claim | null; wait?: Wait }> => {
      const r = await wire<Claim>(
        "claims",
        "mobile-execution-claims",
        input,
        key,
      );
      return {
        claim: r.value,
        wait: r.value === null ? (r.meta as unknown as Wait) : undefined,
      };
    },
    getClaim: (id: UUID) =>
      one<Claim>(claimPath(id), "mobile-execution-claims"),
    renew: (
      id: UUID,
      input: Fence & { binding_revision: number },
      key: string,
    ) =>
      one<Claim>(
        `${claimPath(id)}/renewals`,
        "mobile-execution-claims",
        input,
        key,
      ),
    prepare: (id: UUID, input: PreparationInput, key: string) =>
      one<Preparation>(
        `${claimPath(id)}/preparations`,
        "mobile-execution-preparations",
        input,
        key,
      ),
    permit: (id: UUID, input: PermitInput, key: string) =>
      one<Permit>(
        `${claimPath(id)}/permits`,
        "mobile-send-permits",
        input,
        key,
      ),
    result: (id: UUID, input: ResultInput, key: string) =>
      one<ResultAcknowledgement>(
        `${claimPath(id)}/results`,
        "mobile-delivery-items",
        input,
        key,
      ),
    abort: (id: UUID, input: Fence & { reason: string }, key: string) =>
      one<MobileItem>(
        `${claimPath(id)}/aborts`,
        "mobile-delivery-items",
        input,
        key,
      ),
  };
}

/** Shared JSON:API transport, resource decoding and error handling. */
export interface ApiFailure {
  errors: Array<{
    status?: string;
    code?: string;
    title?: string;
    detail?: string;
    meta?: Record<string, unknown>;
  }>;
}

export function failureDetails(value: unknown): {
  code: string;
  message: string;
  meta?: Record<string, unknown>;
} {
  if (
    typeof value === "object" &&
    value !== null &&
    "errors" in value &&
    Array.isArray(value.errors)
  ) {
    const error: unknown = value.errors[0];
    if (typeof error === "object" && error !== null) {
      return {
        meta:
          "meta" in error &&
          typeof error.meta === "object" &&
          error.meta !== null
            ? (error.meta as Record<string, unknown>)
            : undefined,
        code:
          "code" in error && typeof error.code === "string"
            ? error.code
            : "http_error",
        message:
          "detail" in error && typeof error.detail === "string"
            ? error.detail
            : "title" in error && typeof error.title === "string"
              ? error.title
              : "服务暂时不可用",
      };
    }
  }
  return { code: "http_error", message: "服务暂时不可用" };
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
    public readonly meta?: Record<string, unknown>,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export interface ResourceObject {
  type: string;
  id: string;
  attributes: Record<string, unknown>;
}
export interface PageMeta {
  number: number;
  size: number;
  has_more: boolean;
}
export interface Collection<T> {
  items: T[];
  page?: PageMeta;
}
export interface ResourceDocument {
  data: ResourceObject | ResourceObject[] | null;
  meta?: { page?: PageMeta; [key: string]: unknown };
}
export function object(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
export function invalidDocument(): never {
  throw new ApiError(
    200,
    "invalid_document",
    "服务返回了不合法的 JSON:API 文档",
  );
}
export function decodeResource(value: unknown): Record<string, unknown> {
  if (
    !object(value) ||
    typeof value.type !== "string" ||
    !value.type ||
    typeof value.id !== "string" ||
    !value.id ||
    !object(value.attributes) ||
    "id" in value.attributes ||
    "type" in value.attributes
  )
    return invalidDocument();
  return { ...value.attributes, id: value.id };
}
/** Explicit aliases for action routes whose target resource differs from the URL leaf. */
export function resourceType(path: string): string {
  const parts = path.split("?")[0].split("/").filter(Boolean);
  if (parts[1] === 'v2' && parts[2] === 'admin') {
    if (parts[3] === 'routing') return parts[4];
    if (parts[3] === 'routing-groups' && parts[5] === 'targets') return 'routing-group-targets';
    if (parts[3] === 'keyword-rule-scopes' && parts[5] === 'publications') return 'keyword-rule-publications';
    if (parts[3] === 'source-routes' && parts[5] === 'receipts' && parts[7] === 'rechecks') return 'source-receipt-rechecks';
    if (parts[3] === 'source-routes' && parts[5]) {
      const types:Record<string,string>={configuration:'source-route-configurations',lifecycle:'source-route-lifecycle',managers:'source-route-managers',handoff:'source-route-handoffs'};
      if (types[parts[5]]) return types[parts[5]];
    }
    if (['source-routes','source-credentials'].includes(parts[3])) return parts[5] === 'routes' ? 'source-key-routes' : parts[5] === 'secret-reveals' ? 'source-credential-secret-reveals' : parts[3];
  }
  if (parts[2] === 'wecom-distribution' && parts[3] === 'workspaces') {
    if (!parts[5]) return 'wecom-workspaces';
    if (parts[5] === 'members') return 'wecom-workspace-members';
    const kind = parts[5];
    if (kind === 'groups' && parts[6] === 'from-directory') return 'wecom-group-bindings';
    if (kind === 'broadcast-tasks' && parts[7] === 'preview') return 'broadcast-previews';
    if (['groups','broadcast-tasks','template-previews','template-publications','official-usage-previews','official-usages','broadcast-task-previews','broadcast-access-requests','broadcast-issues','member-policy-previews','member-policies','official-channel-controls'].includes(kind)) return kind;
    throw new Error('Unknown wecom distribution write resource path');
  }
  if (parts[2] === 'platform' && parts[3] === 'wecom-distribution' && parts[4] === 'workspaces') return 'wecom-senders';
  if (parts[2] === 'platform' && parts[3] === 'wecom-distribution' && parts[4] === 'operators') return 'wecom-bindings';
  if (parts[2] === 'platform' && parts[3] === 'administrators') return 'administrators';
  if(parts[0]==='api' && parts[1]==='v10' && parts[2]==='device-execution') {
    const types:Record<string,string>={'task-inbox':'mobile-shared-task-inboxes','display-receipts':'mobile-shared-task-receipts','authority-context':'mobile-shared-delegation-contexts','task-context':'mobile-shared-contexts','idle-telemetry':'mobile-shared-runtime','authorization-decisions':'mobile-shared-delegation-decisions'};
    return types[parts[3]] ?? parts[3];
  }
  const productKind=parts[2]==='platform'?parts[3]:undefined;
  if(productKind && ['template-previews','template-publications','official-usage-previews','official-usages','broadcast-task-previews','broadcast-access-requests','broadcast-issues','member-policy-previews','member-policies','official-channel-controls'].includes(productKind)) return productKind;
  if (parts[2] === "mobile-execution" && parts[3] === "workspaces" && !parts[5]) return "mobile-workspaces";
  if (parts[2] === "mobile-execution" && parts[3] === "workspaces" && parts[5] === "members") return "mobile-workspace-members";
  if (parts[2] === "mobile-execution" || parts[3] === "mobile-execution" || parts[4] === "mobile-execution") {
    const rawKind = parts[2] === "platform" ? parts[4] : parts[5];
    const kind=rawKind?.replace(/^shared-v2-/,"shared-");
    if(rawKind==="shared-v2-support")return "mobile-v9-support";
    if(rawKind==="shared-v2-management")return "mobile-v9-management";
    switch (kind) {
      case "usage-options": return "mobile-usage-options";
      case "usage-previews": return "mobile-usage-previews";
      case "usage-requests": return "mobile-usages";
      case "usages": return parts[7]==="issues"?"mobile-usage-issues":"mobile-usages";
      case "usage-task-previews": return "mobile-usage-task-previews";
      case "shared-task-drafts": return "mobile-shared-drafts";
      case "shared-task-previews": return "mobile-shared-previews";
      case "shared-tasks": return parts[7] === "stops" ? "mobile-shared-stops" : "mobile-shared-tasks";
      case "shared-items": return parts[7] === "reassignments" ? "mobile-shared-reassignments" : "mobile-shared-reviews";
      case "shared-inspections": return "mobile-shared-inspections";
      case "shared-send-policies": return "mobile-shared-policies";
      case "targets": return "mobile-targets";
      case "target-bindings": return "mobile-target-bindings";
      case "devices": return "mobile-devices";
      default: throw new Error("Unknown mobile execution resource path");
    }
  }
  if (parts[0] !== "api" || parts[1] !== "v1")
    throw new Error("Unknown resource path");
  if (
    parts[2] === "platform" &&
    parts[3] === "wecom-assets" &&
    parts[4] === "owner-scope-decisions"
  )
    return "wecom-owner-scope-decisions";
  if (
    parts[2] === "platform" &&
    parts[3] === "wecom-assets" &&
    parts[4] === "sync-runs"
  )
    return "wecom-asset-sync-runs";
  if (parts[2] === "platform" && parts[3] === "execution-devices") {
    if (parts[5] === "enrollments") return "device-enrollments";
    if (parts[5] === "control") return "device-control-commands";
    return "execution-devices";
  }
  if (parts[2] === "platform" && ["distribution-source-policies", "distribution-resource-policies"].includes(parts[3])) return parts[3];
  if (parts[2] === "platform" && parts[3] === "business-requests") return "business-requests";
  if (parts[2] === "auth") return "sessions";
  if (
    parts[2] === "platform" &&
    parts[3] === "wecom-directory" &&
    parts[4] === "credential-rotations"
  )
    return "wecom-credential-rotations";
  if (parts[2] === "profile") return "accounts";
  throw new Error("Unknown write resource path");
}
export function writeDocument(path: string, body: unknown) {
  if (!object(body)) throw new Error("Resource attributes must be an object");
  const { id, ...attributes } = body;
  if (
    "type" in attributes ||
    (id !== undefined && (typeof id !== "string" || !id))
  )
    throw new Error("Invalid resource identity");
  return {
    data: {
      type: resourceType(path),
      ...(id === undefined ? {} : { id }),
      attributes,
    },
  };
}

export function decodeData<T>(document: ResourceDocument): T {
  return (
    Array.isArray(document.data)
      ? document.data.map(decodeResource)
      : document.data === null
        ? null
        : decodeResource(document.data)
  ) as T;
}

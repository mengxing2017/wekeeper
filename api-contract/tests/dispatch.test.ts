import { afterEach, expect, test, vi } from "vitest";
import {
  mobileDeliveryApi as api,
  type DispatchBatch,
} from "../src/modules/mobile-delivery";
import { createDeviceExecutionClient } from "../src/modules/device-execution";
import { resourceType, setCsrf } from "../src/http";
import plan from "../fixtures/automation-dispatch-plan-v5.json";
import canonical from "../fixtures/automation-canonical-v5.json";
const hash = async (s: string) =>
  Array.from(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s)),
    ),
    (b) => b.toString(16).padStart(2, "0"),
  ).join("");
const response = (type: string, attributes: unknown) =>
  new Response(JSON.stringify({ data: { type, id: "fixture", attributes } }), {
    headers: { "Content-Type": "application/vnd.api+json" },
  });
afterEach(() => {
  vi.unstubAllGlobals();
  setCsrf("");
});
test("v5 business writes have explicit resource types and fixed preview confirmation", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(response("automation-dispatch-batches", {}));
  vi.stubGlobal("fetch", fetcher);
  setCsrf("csrf");
  await api.dispatch.confirm(
    "school",
    { id: "preview", preview_digest: "fixed" } as DispatchBatch,
    "confirmation-1",
  );
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual({
    data: {
      type: "automation-dispatch-batches",
      attributes: { preview_digest: "fixed", reviewed: true },
    },
  });
  expect(fetcher.mock.calls[0][1]?.headers).toMatchObject({
    "X-CSRF-Token": "csrf",
    "Idempotency-Key": "confirmation-1",
  });
  for (const [path, type] of [
    [
      "/api/v1/platform/automation-pacing-profiles",
      "automation-pacing-profiles",
    ],
    [
      "/api/v1/platform/automation-flows/f/releases",
      "automation-flow-releases",
    ],
    ["/api/v1/schools/s/automation-device-groups", "automation-device-groups"],
    [
      "/api/v1/schools/s/automation-execution-assignments",
      "automation-execution-assignments",
    ],
    [
      "/api/v1/schools/s/automation-dispatch-settings",
      "automation-dispatch-settings",
    ],
    [
      "/api/v1/schools/s/automation-dispatch-batches/b/remaining-previews",
      "automation-dispatch-batches",
    ],
    [
      "/api/v1/schools/s/automation-flow-validations/v/cancellations",
      "automation-flow-validations",
    ],
  ])
    expect(resourceType(path)).toBe(type);
});
test("v5 normal heartbeat is version-free and rejects the legacy upload methods", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(response("device-heartbeats", {}));
  const client = createDeviceExecutionClient({
    baseUrl: "https://keeper.test",
    protocolVersion: 5,
    token: "device-test",
    fetch: fetcher,
  });
  await client.dispatchHeartbeat(
    {
      binding_revision: 1,
      sender_account_id: "account",
      permissions_ready: false,
    },
    "heartbeat-1",
  );
  expect(fetcher.mock.calls[0][0]).toBe(
    "https://keeper.test/api/v5/device-execution/heartbeats",
  );
  expect(fetcher.mock.calls[0][1]).toMatchObject({
    credentials: "omit",
    redirect: "error",
  });
  expect(
    JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data.attributes,
  ).toEqual({
    binding_revision: 1,
    sender_account_id: "account",
    permissions_ready: false,
  });
  expect(() => client.heartbeat({} as never, "bad")).toThrow("version-free");
  expect(() => client.reportEnvironment({} as never, "bad")).toThrow("v4");
  expect(() => client.prepare("claim", {} as never, "bad")).toThrow(
    "version-free",
  );
  expect(fetcher).toHaveBeenCalledTimes(1);
});
test("shared plan fixtures have canonical hashes and no mandatory installed environment", async () => {
  const sorted = (v: unknown): unknown =>
    Array.isArray(v)
      ? v.map(sorted)
      : v !== null && typeof v === "object"
        ? Object.fromEntries(
            Object.entries(v)
              .sort(([a], [b]) => {
                const x = Array.from(a, (c) => c.codePointAt(0)!);
                const y = Array.from(b, (c) => c.codePointAt(0)!);
                for (let i = 0; i < Math.min(x.length, y.length); i++)
                  if (x[i] !== y[i]) return x[i] - y[i];
                return x.length - y.length;
              })
              .map(([k, v]) => [k, sorted(v)]),
          )
        : v;
  const encode = (v: unknown) => JSON.stringify(sorted(v));
  expect(await hash(encode(plan.body))).toBe(plan.content_digest);
  expect(encode(canonical.value)).toBe(canonical.canonical);
  expect(await hash(canonical.canonical)).toBe(canonical.sha256);
  expect(plan.body.flow.configuration.rules.client).toEqual({
    package_name: "com.wekeeper.fixture.wecom",
  });
  expect(plan.body).not.toHaveProperty("environment_id");
  expect(plan.body).not.toHaveProperty("app_binding_id");
});

test("setup status uses dedicated device credentials before any grant", async () => {
  const fixture = (await import("../fixtures/device-setup-status-v5.json"))
    .default;
  const { id, ...attributes } = fixture;
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      new Response(
        JSON.stringify({
          data: { type: "device-setup-status", id, attributes },
        }),
        { headers: { "Content-Type": "application/vnd.api+json" } },
      ),
    );
  const client = createDeviceExecutionClient({
    baseUrl: "https://keeper.test",
    protocolVersion: 5,
    token: "device-test",
    fetch: fetcher,
  });
  expect(await client.setupStatus()).toEqual(fixture);
  expect(fetcher.mock.calls[0][0]).toBe(
    "https://keeper.test/api/v5/device-execution/setup-status",
  );
  expect(fetcher.mock.calls[0][1]).toMatchObject({
    credentials: "omit",
    redirect: "error",
  });
  expect(fetcher.mock.calls[0][1]?.headers).toMatchObject({
    Authorization: "Bearer device-test",
  });
});


test("school resource projection uses only the school read endpoint", async () => {
  const fixture = (await import("../fixtures/school-execution-resource.json")).default;
  const { id, ...attributes } = fixture;
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({
    data: [{ type: "school-execution-resources", id, attributes }],
  }), { headers: { "Content-Type": "application/vnd.api+json" } }));
  vi.stubGlobal("fetch", fetcher);
  const result = await api.dispatch.resources("school-a", 2);
  expect(result.items).toEqual([fixture]);
  expect(String(fetcher.mock.calls[0][0])).toContain("/schools/school-a/execution-resources");
  expect(String(fetcher.mock.calls[0][0])).toContain("page[number]=2");
  expect(fetcher).toHaveBeenCalledTimes(1);
});

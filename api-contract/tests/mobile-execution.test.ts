import { afterEach, expect, test, vi } from "vitest";
import { createDeviceExecutionClient } from "../src/modules/device-execution";
import { mobileDeliveryApi } from "../src/modules/mobile-delivery";
import { ApiError, setCsrf } from "../src/http";
import permit from "../fixtures/mobile-send-permit.json";
import wait from "../fixtures/mobile-execution-wait.json";
import notClicked from "../fixtures/mobile-execution-not-clicked.json";
afterEach(() => {
  vi.unstubAllGlobals();
  setCsrf("");
});
const response = (v: unknown, status = 200) =>
  new Response(JSON.stringify(v), {
    status,
    headers: { "Content-Type": "application/vnd.api+json" },
  });
test("device transport uses fixed HTTPS, no cookie, CSRF, redirect or automatic retry", async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response(permit));
  setCsrf("browser-only");
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    token: "dedicated-token",
    fetch: fetcher,
  });
  const result = await client.permit(
    "claim",
    { fence: 4, preparation_id: "prep", snapshot_digest: "snapshot-sha256" },
    "stable-key",
  );
  expect(result.invocation_id).toBe(permit.data.attributes.invocation_id);
  expect(fetcher).toHaveBeenCalledTimes(1);
  const [url, init] = fetcher.mock.calls[0];
  expect(url).toBe(
    "https://example.test/api/v1/device-execution/claims/claim/permits",
  );
  expect(init).toMatchObject({
    credentials: "omit",
    redirect: "error",
    cache: "no-store",
  });
  expect(init?.headers).toMatchObject({
    Authorization: "Bearer dedicated-token",
    "Idempotency-Key": "stable-key",
  });
  expect(init?.headers).not.toHaveProperty("X-CSRF-Token");
  expect(JSON.parse(String(init?.body))).toEqual({
    data: {
      type: "mobile-send-permits",
      attributes: {
        fence: 4,
        preparation_id: "prep",
        snapshot_digest: "snapshot-sha256",
      },
    },
  });
  for (const baseUrl of [
    "http://example.test",
    "https://user:pass@example.test",
    "https://example.test/path",
    "https://example.test/?token=x",
  ])
    expect(() => createDeviceExecutionClient({ baseUrl })).toThrow();
});
test("short-code enrollment does not require a device ID or browser credentials", async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response({
    data: { type: "device-credentials", id: "device", attributes: {
      device_token: "test-token", credential_generation: 1, protocol_version: 3,
      expires_at: "2026-10-30T00:00:00Z",
    } },
  }));
  const client = createDeviceExecutionClient({ baseUrl: "https://example.test", protocolVersion: 3, fetch: fetcher });
  const result = await client.enroll({ code: "00123456", client_instance_id: "phone", protocol_version: 3 }, "pair-key");
  expect(result.id).toBe("device");
  const [, init] = fetcher.mock.calls[0];
  expect(init).toMatchObject({ credentials: "omit", redirect: "error" });
  expect(JSON.parse(String(init?.body)).data.attributes).toEqual({ code: "00123456", client_instance_id: "phone", protocol_version: 3 });
  expect(init?.headers).not.toHaveProperty("Authorization");
  expect(fetcher).toHaveBeenCalledTimes(1);
});
test("normal no-task wait preserves null and its reason", async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response(wait));
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    fetch: fetcher,
  });
  expect(await client.claim({ binding_revision: 2 }, "claim-key")).toEqual({
    claim: null,
    wait: wait.meta,
  });
});
test("device errors retain metadata and never retry ambiguous requests", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      response(
        {
          errors: [
            {
              code: "unknown_requires_review",
              detail: "review",
              meta: { next_eligible_at: null },
            },
          ],
        },
        409,
      ),
    );
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    fetch: fetcher,
  });
  await expect(
    client.permit(
      "claim",
      { fence: 4, preparation_id: "prep", snapshot_digest: "digest" },
      "key",
    ),
  ).rejects.toMatchObject({
    code: "unknown_requires_review",
    meta: { next_eligible_at: null },
  });
  expect(fetcher).toHaveBeenCalledTimes(1);
});
test("management aliases use existing browser CSRF transport and explicit JSON:API types", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      response({
        data: { type: "mobile-delivery-previews", id: "task", attributes: {} },
      }),
    );
  vi.stubGlobal("fetch", fetcher);
  setCsrf("csrf");
  await mobileDeliveryApi.preview(
    "school",
    "task",
    { expected_revision: 1 },
    "preview-key",
  );
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data.type).toBe(
    "mobile-delivery-previews",
  );
  expect(fetcher.mock.calls[0][1]?.headers).toMatchObject({
    "X-CSRF-Token": "csrf",
  });
  expect(() =>
    mobileDeliveryApi.preview("school", "task", { expected_revision: 1 }, ""),
  ).toThrow();
  expect(new ApiError(409, "test", "test", { reason: "test" }).meta).toEqual({
    reason: "test",
  });
});

test("not-clicked result keeps a null click timestamp and its stable event key", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      response({
        data: {
          type: "mobile-delivery-items",
          id: "item",
          attributes: { state: "unknown" },
        },
      }),
    );
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    token: "device-only",
    fetch: fetcher,
  });
  await client.result(
    "claim",
    {
      ...notClicked.data.attributes,
      result: "not_observed",
      local_state: "not_observed",
    },
    "stable-result-key",
  );
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual(
    notClicked,
  );
  expect(fetcher.mock.calls[0][1]?.headers).toMatchObject({
    "Idempotency-Key": "stable-result-key",
  });
  expect(fetcher).toHaveBeenCalledTimes(1);
});

test("visible v2 claims keep names, profiles and device target without external identifiers", async () => {
  const { default: fixture } = await import(
    "../fixtures/mobile-visible-claim-v2.json"
  );
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response(fixture));
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    protocolVersion: 2,
    fetch: fetcher,
  });
  const { claim } = await client.claim({ binding_revision: 2 }, "v2-stable");
  expect(fetcher.mock.calls[0][0]).toBe(
    "https://example.test/api/v2/device-execution/claims",
  );
  expect(claim?.group_snapshot).toMatchObject({
    name: "机器人测试",
    identity_mode: "ui_visible",
    profile_revision: 3,
  });
  expect(claim?.sender_binding).not.toHaveProperty("external_key");
  expect(claim?.group_snapshot).not.toHaveProperty("identity_namespace");
  expect(claim?.execution_target).toEqual(
    fixture.data.attributes.execution_target,
  );
});

test("strategy run has explicit type, expected revision and stable replay key", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      response({
        data: {
          type: "mobile-delivery-strategy-runs",
          id: "run",
          attributes: { task_id: "draft" },
        },
      }),
    );
  vi.stubGlobal("fetch", fetcher);
  await mobileDeliveryApi.runStrategy(
    "school",
    "strategy",
    { expected_revision: 7 },
    "fixed-run",
  );
  expect(fetcher.mock.calls[0][0]).toBe(
    "/api/v1/schools/school/mobile-delivery-strategies/strategy/runs",
  );
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data).toEqual({
    type: "mobile-delivery-strategy-runs",
    attributes: { expected_revision: 7 },
  });
  expect(fetcher.mock.calls[0][1]?.headers).toMatchObject({
    "Idempotency-Key": "fixed-run",
  });
});

test("configured v3 uses its own transport and retains immutable rules and sequence", async () => {
  const { default: fixture } = await import(
    "../fixtures/mobile-configured-claim-v3.json"
  );
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response(fixture));
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    protocolVersion: 3,
    fetch: fetcher,
  });
  const result = await client.claim({ binding_revision: 2 }, "v3-claim");
  expect(fetcher.mock.calls[0][0]).toBe(
    "https://example.test/api/v3/device-execution/claims",
  );
  expect(result.claim?.sequence).toEqual(fixture.data.attributes.sequence);
  expect(result.claim?.configuration).toEqual(
    fixture.data.attributes.configuration,
  );
  expect(result.claim?.requirements.protocol_version).toBe(3);
});

test("ordered strategy preserves explicit order, interval and published release", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      response({
        data: {
          type: "mobile-delivery-strategies",
          id: "strategy",
          attributes: {},
        },
      }),
    );
  vi.stubGlobal("fetch", fetcher);
  const input = {
    name: "ordered",
    device_id: "device",
    sender_account_id: "sender",
    delivery_mode: "ordered_groups" as const,
    group_id: null,
    targets: [{ group_id: "b" }, { group_id: "a" }],
    inter_group_interval_seconds: 120,
    configuration_release_id: "release",
    text_content: "测试",
    trigger_mode: "manual_once" as const,
    enabled: false,
  };
  await mobileDeliveryApi.createStrategy("school", input, "create-ordered");
  expect(
    JSON.parse(String(fetcher.mock.calls[0][1]?.body)).data.attributes,
  ).toEqual(input);
});

import { afterEach, expect, test, vi } from "vitest";
import {
  mobileDeliveryApi as api,
  type AutomationPreview,
  type EnvironmentReport,
  type ValidationEvent,
} from "../src/modules/mobile-delivery";
import { createDeviceExecutionClient } from "../src/modules/device-execution";
import { resourceType, setCsrf } from "../src/http";
import environment from "../fixtures/automation-environment-v4.json";
import validation from "../fixtures/automation-validation-v4.json";
const response = (type: string, attributes: unknown, id = "resource") =>
  new Response(JSON.stringify({ data: { type, id, attributes } }), {
    headers: { "Content-Type": "application/vnd.api+json" },
  });
afterEach(() => {
  vi.unstubAllGlobals();
  setCsrf("");
});
test("automation uses typed resource aliases, CSRF and stable confirmation identity", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      response("automation-business-confirmations", { task_id: "task" }),
    );
  vi.stubGlobal("fetch", fetcher);
  setCsrf("session-csrf");
  const preview = {
    id: "preview",
    revision: 3,
    preview_digest: "frozen-digest",
  } as AutomationPreview;
  await api.automation.confirm("school", preview, "same-key");
  const [url, init] = fetcher.mock.calls[0];
  expect(url).toBe(
    "/api/v1/schools/school/automation-business-previews/preview/confirmations",
  );
  expect(init?.headers).toMatchObject({
    "X-CSRF-Token": "session-csrf",
    "Idempotency-Key": "same-key",
  });
  expect(JSON.parse(String(init?.body))).toEqual({
    data: {
      type: "automation-business-confirmations",
      attributes: {
        expected_revision: 3,
        preview_digest: "frozen-digest",
        reviewed: true,
      },
    },
  });
  expect(
    resourceType(
      "/api/v1/platform/automation-adaptations/components/c/releases",
    ),
  ).toBe("automation-releases");
  expect(
    resourceType("/api/v1/platform/automation-adaptations/components"),
  ).toBe("automation-components");
  expect(resourceType("/api/v1/schools/s/device-app-bindings")).toBe(
    "device-app-bindings",
  );
  expect(
    resourceType("/api/v1/schools/s/automation-validations/v/cancellations"),
  ).toBe("automation-validations");
});
test("v4 environment and fixed validation events never use browser credentials or rewrite older protocols", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(response("device-environments", {}));
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    token: "wk_device_test",
    protocolVersion: 4,
    fetch: fetcher,
  });
  await client.reportEnvironment(
    environment.data.attributes.report as EnvironmentReport,
    "env-1",
  );
  expect(fetcher.mock.calls[0][0]).toBe(
    "https://example.test/api/v4/device-execution/environment-snapshots",
  );
  expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual(
    environment,
  );
  expect(fetcher.mock.calls[0][1]).toMatchObject({
    redirect: "error",
    credentials: "omit",
    cache: "no-store",
  });
  fetcher.mockResolvedValue(response("device-validations", {}));
  const event = validation.data.attributes.event as ValidationEvent;
  await client.validationEvent("job", event, "event-1");
  expect(JSON.parse(String(fetcher.mock.calls[1][1]?.body))).toEqual(
    validation,
  );
  for (const protocolVersion of [1, 2, 3] as const) {
    const old = createDeviceExecutionClient({
      baseUrl: "https://example.test",
      protocolVersion,
      fetch: fetcher,
    });
    expect(() =>
      old.reportEnvironment(
        environment.data.attributes.report as EnvironmentReport,
        "x",
      ),
    ).toThrow("v4");
  }
  expect(fetcher).toHaveBeenCalledTimes(2);
});
test("validation polling accepts only the bounded collection and does not accept a job", async () => {
  const fetcher = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      new Response(
        JSON.stringify({
          data: [
            {
              type: "device-validations",
              id: "job",
              attributes: { state: "queued" },
            },
          ],
        }),
        { headers: { "Content-Type": "application/vnd.api+json" } },
      ),
    );
  const client = createDeviceExecutionClient({
    baseUrl: "https://example.test",
    protocolVersion: 4,
    fetch: fetcher,
  });
  expect(await client.validationJobs()).toMatchObject([
    { id: "job", state: "queued" },
  ]);
  expect(fetcher.mock.calls[0][1]?.method).toBe("GET");
  expect(fetcher.mock.calls[0][1]?.body).toBeUndefined();
  expect(fetcher).toHaveBeenCalledTimes(1);
});

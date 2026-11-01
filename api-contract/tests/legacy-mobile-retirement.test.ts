import { afterEach, expect, test, vi } from "vitest";
import { mobileDeliveryApi as api } from "../src/modules/mobile-delivery";
import fixture from "../fixtures/legacy-mobile-maintenance-retired.json";
afterEach(() => vi.unstubAllGlobals());
test("retired v5 write surfaces stable error without retry or fake success", async () => {
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(fixture), {status:409,headers:{"Content-Type":"application/vnd.api+json"}}));
  vi.stubGlobal("fetch", fetcher);
  await expect(api.dispatch.enabled("school", true, "fixed-key")).rejects.toMatchObject({status:409,code:"legacy_mobile_maintenance_retired",message:fixture.errors[0].detail});
  expect(fetcher).toHaveBeenCalledTimes(1);
});

import { it, expect, vi } from "vitest";
import {
  createDispatchRecordsClient,
  type DispatchEvent,
} from "../src/modules/mobile-execution/shared-send/dispatch-records";
import fixture from "../fixtures/mobile-dispatch-event.json";
it("shares the task-less failure event serialized by Rust", () => {
  const e: DispatchEvent = fixture;
  expect(e.item_id).toBeNull();
  expect(e.reason).toBe("mobile_candidates_missing");
});
it("queries histories with safe ids, independent cursors and precise filters", async () => {
  const requestResource = vi.fn().mockResolvedValue({ items: [] });
  const c = createDispatchRecordsClient({ requestResource });
  await c.list({
    source_kind: "all",
    reason: "device_busy",
    reason_scope: "history",
    cursor: "cursor",
    page_size: 1,
  });
  expect(requestResource).toHaveBeenLastCalledWith(
    "/api/v2/admin/mobile-dispatch/intakes?source_kind=all&reason=device_busy&reason_scope=history&cursor=cursor&page_size=1",
  );
  await c.events("intake/id", { cursor: "12" });
  expect(requestResource).toHaveBeenLastCalledWith(
    "/api/v2/admin/mobile-dispatch/intakes/intake%2Fid/events?cursor=12",
  );
  await c.exchangeChecks("id");
  expect(requestResource).toHaveBeenLastCalledWith(
    "/api/v2/admin/mobile-dispatch/delivery-exchanges/id/checks?",
  );
  expect("retry" in c).toBe(false);
  expect("send" in c).toBe(false);
});

import {afterEach,expect,test,vi} from 'vitest';
import {deviceRegistryApi} from '../src/modules/device-registry';
import {setCsrf} from '../src/http';
afterEach(()=>{vi.unstubAllGlobals();setCsrf('')});
const response=(v:unknown,status=200)=>new Response(JSON.stringify(v),{status,headers:{'Content-Type':'application/vnd.api+json'}});
test("device control presence and commands use the authenticated platform client", async () => {
  const fixture = (await import("../fixtures/device-control-frame.json")).default;
  const frame: import("../src/modules/device-registry").DeviceControlFrame = { ...fixture, protocol_version: 1 };
  expect(frame.heartbeat_interval_seconds).toBe(60);
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => response({ data: { type: "device-control-commands", id: "command", attributes: { kind: "pause_execution", state: "queued" } } }));
  vi.stubGlobal("fetch", fetcher);
  await deviceRegistryApi.controlCommand("phone", "pause_execution", "pause-once");
  expect(fetcher.mock.calls[0][0]).toBe("/api/v1/platform/execution-devices/phone/control");
  expect(JSON.parse(fetcher.mock.calls[0][1]?.body as string).data.attributes).toEqual({ kind: "pause_execution" });
  expect(JSON.parse(fetcher.mock.calls[0][1]?.body as string).data.type).toBe("device-control-commands");
  for (const kind of ["request_status", "resume_execution"] as const) {
    await deviceRegistryApi.controlCommand("phone", kind, `${kind}-once`);
    const call = fetcher.mock.calls.at(-1)!;
    expect(call[0]).toBe("/api/v1/platform/execution-devices/phone/control");
    expect(JSON.parse(call[1]?.body as string)).toEqual({ data: { type: "device-control-commands", attributes: { kind } } });
    expect(new Headers(call[1]?.headers).get("Idempotency-Key")).toBe(`${kind}-once`);
  }

});

test("batched device list preserves the shared control presence projection", async () => {
  const fixture = (await import("../fixtures/device-control-presence.json")).default;
  const snapshot: import("../src/modules/device-registry").DeviceControlSnapshot = { ...fixture, state: "connecting", transport: "https" };
  const fetcher = vi.fn<typeof fetch>().mockResolvedValue(response({ data: [{ type: "execution-devices", id: fixture.id, attributes: { name: "phone", control: snapshot } }] }));
  vi.stubGlobal("fetch", fetcher);
  const page = await deviceRegistryApi.devices();
  expect(page.items[0].control).toEqual(snapshot);
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(snapshot.last_control_heartbeat_at).toBeNull();
});

test("platform heartbeat probes and responses preserve correlation and session identity", async () => {
  const fixture = (await import("../fixtures/device-control-probe.json")).default;
  const frame: import("../src/modules/device-registry").DeviceControlFrame = { ...fixture.frame, protocol_version: 1 };
  const pulse: import("../src/modules/device-registry").DeviceControlPulse = fixture.response;
  expect(pulse.probe_id).toBe(frame.probe_id);
  expect(pulse.connection_id).toBe(frame.connection_id);
  expect(pulse.epoch).toBe(frame.epoch);
  expect(pulse.heartbeat_id).not.toBe(frame.probe_id);
  expect(pulse.heartbeat_sequence).toBe(1);
});

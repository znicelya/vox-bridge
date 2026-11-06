import { expect, it, vi } from "vitest";
import { desktopPort, invokeDecoded, localDesktopError } from "./desktop";
import { decodeSessionRecord } from "./wire";
import fixture from "../../tests/contracts/runtime-wire.json";
const tauri = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
it("unwraps only the Tauri event payload and forwards command envelopes", async () => {
  tauri.invoke.mockResolvedValue(fixture.callRecord);
  const unlisten = vi.fn();
  tauri.listen.mockImplementation(async (_name, callback) => {
    callback({ event: "named", id: 9, payload: { type: "resync_needed" } });
    return unlisten;
  });
  expect(
    await invokeDecoded(
      desktopPort,
      "get_session",
      { sessionId: fixture.callRecord.id },
      decodeSessionRecord,
    ),
  ).toEqual(fixture.callRecord);
  expect(tauri.invoke).toHaveBeenCalledWith("get_session", {
    sessionId: fixture.callRecord.id,
  });
  const received: unknown[] = [];
  const off = await desktopPort.listen("named", (payload) =>
    received.push(payload),
  );
  off();
  expect(received).toEqual([{ type: "resync_needed" }]);
  expect(unlisten).toHaveBeenCalledOnce();
});
it("reports safe local decode and rejection errors", async () => {
  tauri.invoke.mockResolvedValue({ api_key: "must-never-render" });
  await expect(
    invokeDecoded(desktopPort, "get_session", {}, decodeSessionRecord),
  ).rejects.toThrow("桌面数据格式无效");
  expect(localDesktopError({ message: "must-never-render" })).not.toContain(
    "must-never-render",
  );
  expect(localDesktopError("[STORAGE_ERROR] must-never-render")).not.toContain(
    "must-never-render",
  );
});

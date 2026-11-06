import "@/test/workbench-dom";
import {
  cleanup,
  render,
  renderHook,
  screen,
  waitFor,
  act,
  fireEvent,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { useWorkbench } from "./useWorkbench";
import {
  workbenchDesktopFixture,
  workbenchWire,
} from "@/test/workbench-fixture";
const user = () => userEvent.setup();
async function choose(
  u: ReturnType<typeof user>,
  name: string,
  option: string,
) {
  await u.click(await screen.findByRole("combobox", { name }));
  await u.click(await screen.findByRole("option", { name: option }));
}
async function routes(u: ReturnType<typeof user>, call = true) {
  if (call) await u.click(screen.getByRole("tab", { name: "通话" }));
  await choose(u, "同传服务", "测试同传服务");
  await choose(u, call ? "我的麦克风 A" : "音频输入", "Microphone");
  await choose(u, "M1 · VoxBridge Input", "VoxBridge Input");
  if (call) {
    await choose(u, "M2 · VoxBridge Output", "VoxBridge Output");
    await choose(u, "我的扬声器", "Speakers");
  }
}
beforeEach(() => {
  window.location.hash = "#workbench";
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
describe("live and call workbench", () => {
  it("launches visible call routes and resumes only the failed direction", async () => {
    const port = workbenchDesktopFixture(),
      u = user();
    render(<App desktop={port} />);
    await routes(u);
    await u.click(screen.getByRole("button", { name: "开始同传" }));
    await waitFor(() =>
      expect(port.lastArgs("create_session")).toEqual({
        launch: workbenchWire.callLaunch,
      }),
    );
    await waitFor(() => expect(port.liveListenerCount()).toBe(1));
    act(() => {
      port.setSession({
        ...workbenchWire.callRecord,
        status: "running",
        runtime: {
          phase: "running",
          outcome: null,
          directions: [
            {
              direction: "source_to_target",
              phase: "running",
              failure: null,
              stream_epoch: 0,
            },
            {
              direction: "target_to_source",
              phase: "paused",
              failure: "provider",
              stream_epoch: 1,
            },
          ],
        },
      });
      port.emit({
        type: "audio_state",
        event_id: "00000000-0000-0000-0000-000000000019",
        session_id: workbenchWire.callRecord.id,
        occurred_at: { secs_since_epoch: 1, nanos_since_epoch: 0 },
        state: { state: "paused", reason: "offline" },
      });
    });
    await u.click(await screen.findByRole("button", { name: "继续对方到我" }));
    expect(port.lastArgs("resume_session_direction")).toEqual({
      sessionId: workbenchWire.callRecord.id,
      direction: "target_to_source",
    });
    await u.click(screen.getByText("语言与音频配置"));
    expect(screen.getByRole("tab", { name: "直播" })).toBeDisabled();
  });
  it("queues one Stop during delayed create and keeps it reachable after navigation", async () => {
    const port = workbenchDesktopFixture(),
      u = user(),
      gate = port.deferNext("create_session");
    render(<App desktop={port} />);
    await routes(u);
    await u.dblClick(screen.getByRole("button", { name: "开始同传" }));
    await gate.requested;
    await u.click(screen.getByRole("link", { name: "设置" }));
    await u.click(screen.getByRole("button", { name: "停止同传" }));
    expect(port.calls.filter((x) => x.command === "stop_session")).toHaveLength(
      0,
    );
    gate.release();
    await waitFor(() =>
      expect(
        port.calls.filter((x) => x.command === "stop_session"),
      ).toHaveLength(1),
    );
    expect(
      port.calls.filter((x) => x.command === "create_session"),
    ).toHaveLength(1);
  });
  it("cancels create when Stop is requested during validation", async () => {
    const port = workbenchDesktopFixture(),
      u = user(),
      gate = port.deferNext("validate_session_launch");
    render(<App desktop={port} />);
    await routes(u);
    await u.click(screen.getByRole("button", { name: "开始同传" }));
    await gate.requested;
    await u.click(screen.getByRole("button", { name: "停止同传" }));
    gate.release();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "开始同传" })).toBeEnabled(),
    );
    expect(port.lastArgs("create_session")).toBeUndefined();
  });
  it("preserves session tickets and exact selected row text across navigation without retrieval calls", async () => {
    const port = workbenchDesktopFixture(),
      u = user();
    render(<App desktop={port} />);
    await routes(u);
    await u.click(screen.getByRole("button", { name: "开始同传" }));
    await waitFor(() => expect(port.liveListenerCount()).toBe(1));
    const row = {
      id: "00000000-0000-0000-0000-000000000077",
      session_id: workbenchWire.callRecord.id,
      direction: "source_to_target" as const,
      sequence: 9,
      occurred_at_ms: 1730000000000,
      language: "en",
      text: "  Exact\ntext & spacing  ",
      is_final: true,
      details: null,
    };
    act(() => port.emit({ type: "transcript", update: row }));
    const before = port.calls.length;
    await u.click(await screen.findByRole("button", { name: "用此段检索" }));
    expect(
      screen.getAllByText(row.text, { exact: true, normalizer: (s) => s })
        .length,
    ).toBeGreaterThan(0);
    expect(port.calls.slice(before)).toHaveLength(0);
    await u.click(screen.getByRole("link", { name: "设置" }));
    expect(screen.getByRole("button", { name: "停止同传" })).toBeEnabled();
    await u.click(screen.getByRole("link", { name: "实时工作台" }));
    expect(port.activeTicketCount()).toBe(1);
    expect(
      port.calls.filter((x) => x.command === "subscribe_session"),
    ).toHaveLength(1);
    expect(port.lastArgs("stop_session")).toBeUndefined();
  });
  it("rejects stale endpoints without selecting fallback or creating a session", async () => {
    const port = workbenchDesktopFixture(),
      u = user();
    render(<App desktop={port} />);
    await routes(u);
    port.setEndpoints([]);
    await u.click(screen.getByRole("button", { name: "开始同传" }));
    expect(
      await screen.findByText("音频设备不可用，请检查设备。"),
    ).toBeInTheDocument();
    expect(port.lastArgs("create_session")).toBeUndefined();
    expect(
      screen.getByRole("combobox", { name: "我的麦克风 A" }),
    ).toHaveTextContent("Microphone");
  });
  it("disables unavailable providers and rejects a wrong pair before launch", async () => {
    const port = workbenchDesktopFixture(),
      u = user();
    port.setCapabilities({ ready: false, unavailable_reason: "未联调" });
    render(<App desktop={port} />);
    await u.click(await screen.findByRole("combobox", { name: "同传服务" }));
    expect(
      await screen.findByRole("option", { name: /测试同传服务/ }),
    ).toHaveAttribute("aria-disabled", "true");
    await u.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "开始同传" })).toBeDisabled();
  });
  it("shows an honest desktop-unavailable state without injected inventory", async () => {
    render(
      <App
        desktop={{
          invoke: () => Promise.reject(new Error("Tauri absent")),
          listen: () => Promise.reject(new Error("Tauri absent")),
        }}
      />,
    );
    expect(await screen.findByText(/桌面服务不可用/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始同传" })).toBeDisabled();
  });
});
it("retains the call draft through navigation and releases Stop on its terminal reply", async () => {
  const port = workbenchDesktopFixture(),
    u = user(),
    gate = port.deferNext("stop_session");
  render(<App desktop={port} />);
  await routes(u);
  await u.click(screen.getByRole("button", { name: "开始同传" }));
  await waitFor(() => expect(port.liveListenerCount()).toBe(1));
  await u.click(screen.getByRole("link", { name: "设置" }));
  await u.click(screen.getByRole("link", { name: "实时工作台" }));
  expect(screen.getByRole("tab", { name: "通话" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(
    screen.getByRole("combobox", { name: "我的麦克风 A" }),
  ).toHaveTextContent("Microphone");
  await u.click(screen.getByRole("button", { name: "停止同传" }));
  await gate.requested;
  gate.release();
  await waitFor(() =>
    expect(
      screen.queryByRole("button", { name: "停止同传" }),
    ).not.toBeInTheDocument(),
  );
  expect(screen.getByRole("button", { name: "开始同传" })).toBeEnabled();
});
it("retains native resume ownership after deadline and terminal stop until the original reply settles", async () => {
  const port = workbenchDesktopFixture();
  const { result } = renderHook(() => useWorkbench(port));
  await waitFor(() => expect(result.current.loading).toBe(false));
  await act(async () => {
    await result.current.start(workbenchWire.callLaunch);
  });
  await waitFor(() => expect(port.liveListenerCount()).toBe(1));
  act(() => {
    port.setSession({
      ...workbenchWire.callRecord,
      status: "paused",
      runtime: {
        phase: "paused",
        outcome: null,
        directions: [
          {
            direction: "target_to_source",
            phase: "paused",
            failure: "provider",
            stream_epoch: 1,
          },
        ],
      },
    });
    port.emit({
      type: "audio_state",
      event_id: "00000000-0000-0000-0000-000000000019",
      session_id: workbenchWire.callRecord.id,
      occurred_at: { secs_since_epoch: 1, nanos_since_epoch: 0 },
      state: { state: "paused", reason: "offline" },
    });
  });
  await waitFor(() => expect(result.current.session?.status).toBe("paused"));
  const gate = port.deferNext("resume_session_direction");
  vi.useFakeTimers();
  act(() => {
    void result.current.resume("target_to_source");
  });
  await gate.requested;
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10001);
  });
  vi.useRealTimers();
  expect(result.current.error).toBe("桌面响应超时，请重试。");
  expect(result.current.resuming).toEqual(["target_to_source"]);
  expect(port.pendingNativeCount("resume_session_direction")).toBe(1);
  act(() => result.current.stop());
  await waitFor(() => expect(result.current.session?.status).toBe("failed"));
  expect(result.current.locked).toBe(true);
  gate.release();
  await waitFor(() => expect(result.current.locked).toBe(false));
});
it("preserves real empty destinations while Stop remains available", async () => {
  const port = workbenchDesktopFixture(),
    u = user();
  render(<App desktop={port} />);
  await routes(u);
  await u.click(screen.getByRole("button", { name: "开始同传" }));
  await waitFor(() => expect(port.liveListenerCount()).toBe(1));
  await u.click(screen.getByRole("link", { name: "知识库" }));
  expect(screen.getByText("知识库尚无文档")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "停止同传" })).toBeEnabled();
});
it.each([
  ["知识库", "知识库尚无文档"],
  ["会话历史", "还没有会话记录"],
])(
  "shows rejected Stop feedback on %s without replacing its empty content",
  async (route, empty) => {
    const port = workbenchDesktopFixture(),
      u = user();
    render(<App desktop={port} />);
    await routes(u);
    await u.click(screen.getByRole("button", { name: "开始同传" }));
    await waitFor(() => expect(port.liveListenerCount()).toBe(1));
    await u.click(screen.getByRole("link", { name: route }));
    port.rejectNext("stop_session", "[AUDIO_ERROR] private native details");
    await u.click(screen.getByRole("button", { name: "停止同传" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "音频设备不可用，请检查设备。",
    );
    expect(
      screen.queryByText(/private native details/),
    ).not.toBeInTheDocument();
    expect(screen.getByText(empty)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止同传" })).toBeEnabled();
  },
);
it.each([
  ["知识库", "知识库尚无文档"],
  ["会话历史", "还没有会话记录"],
])(
  "shows timed-out Stop feedback on %s while retaining native ownership",
  async (route, empty) => {
    const port = workbenchDesktopFixture(),
      u = user();
    render(<App desktop={port} />);
    await routes(u);
    await u.click(screen.getByRole("button", { name: "开始同传" }));
    await waitFor(() => expect(port.liveListenerCount()).toBe(1));
    await u.click(screen.getByRole("link", { name: route }));
    const gate = port.deferNext("stop_session");
    vi.useFakeTimers();
    fireEvent.click(screen.getByRole("button", { name: "停止同传" }));
    await gate.requested;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10001);
    });
    vi.useRealTimers();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "桌面响应超时，请重试。",
    );
    expect(screen.getByText(empty)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止同传" })).toBeDisabled();
    expect(port.pendingNativeCount("stop_session")).toBe(1);
    gate.release();
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "停止同传" }),
      ).not.toBeInTheDocument(),
    );
  },
);
it("rejects an unsupported language pair before transport validation", async () => {
  const port = workbenchDesktopFixture(),
    u = user();
  render(<App desktop={port} />);
  await routes(u);
  await choose(u, "原文语言", "日本語");
  await u.click(screen.getByRole("button", { name: "开始同传" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "不支持所选语言方向",
  );
  expect(port.lastArgs("validate_session_launch")).toBeUndefined();
});
it("captures partial text exactly with its segment identity and no transport command", async () => {
  const port = workbenchDesktopFixture(),
    u = user();
  render(<App desktop={port} />);
  await routes(u);
  await u.click(screen.getByRole("button", { name: "开始同传" }));
  await waitFor(() => expect(port.liveListenerCount()).toBe(1));
  const row = {
    id: "00000000-0000-0000-0000-000000000078",
    session_id: workbenchWire.callRecord.id,
    direction: "source_to_target" as const,
    sequence: 10,
    occurred_at_ms: 1730000000000,
    language: "en",
    text: "alpha  exact\n beta",
    is_final: true,
    details: null,
  };
  act(() => port.emit({ type: "transcript", update: row }));
  const text = await screen.findByText(row.text, {
      exact: true,
      normalizer: (s) => s,
    }),
    range = document.createRange();
  range.setStart(text.firstChild!, 5);
  range.setEnd(text.firstChild!, 13);
  window.getSelection()!.removeAllRanges();
  window.getSelection()!.addRange(range);
  expect(window.getSelection()!.toString()).toBe("  exact\n");
  fireEvent.mouseUp(text);
  const before = port.calls.length;
  await u.click(await screen.findByRole("button", { name: "检索所选内容" }));
  expect(screen.getByRole("region", { name: "检索上下文" })).toHaveTextContent(
    "exact",
  );
  expect(port.calls.slice(before)).toHaveLength(0);
});
it("coalesces timed-out native reloads and serializes capabilities without publishing stale reads", async () => {
  const port = workbenchDesktopFixture();
  const configs = Array.from({ length: 20 }, (_, i) => ({
    config: {
      id: `00000000-0000-0000-0000-${(200 + i).toString().padStart(12, "0")}`,
      provider: "volcengine",
      display_name: `离线配置 ${i}`,
      endpoint: "wss://openspeech.bytedance.com/api/v4/ast/v2/translate",
      llm: null,
      embedding: null,
      tavily: null,
      language_capabilities: [],
      secret_reference: null,
    },
    credential_configured: true,
  }));
  port.setProfiles(configs);
  const first = port.deferNext("list_audio_endpoints");
  vi.useFakeTimers();
  const { result } = renderHook(() => useWorkbench(port));
  await first.requested;
  for (let i = 0; i < 3; i++) {
    act(() => {
      void result.current.reload();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10001);
    });
  }
  expect(port.pendingNativeCount("list_audio_endpoints")).toBe(1);
  expect(
    port.calls.filter((x) => x.command === "list_audio_endpoints"),
  ).toHaveLength(1);
  expect(
    port.calls.filter((x) => x.command === "provider_capabilities"),
  ).toHaveLength(0);
  port.setEndpoints([]);
  act(() => {
    void result.current.reload({ fresh: true });
  });
  const cap = port.deferNext("provider_capabilities");
  first.release();
  await cap.requested;
  expect(port.pendingNativeCount("provider_capabilities")).toBe(1);
  expect(
    port.calls.filter((x) => x.command === "provider_capabilities"),
  ).toHaveLength(1);
  expect(result.current.inventory).toBeNull();
  cap.release();
  await act(async () => {
    await Promise.resolve();
  });
  vi.useRealTimers();
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.inventory?.endpoints).toEqual([]);
  expect(Object.keys(result.current.capabilities)).toHaveLength(20);
});
it("collapses active configuration so captions remain primary and preserves inspectable disabled routes", async () => {
  const port = workbenchDesktopFixture(),
    u = user();
  render(<App desktop={port} />);
  await routes(u);
  await u.click(screen.getByRole("button", { name: "开始同传" }));
  await waitFor(() => expect(port.liveListenerCount()).toBe(1));
  const disclosure = screen.getByText("语言与音频配置").closest("details")!;
  expect(disclosure).not.toHaveAttribute("open");
  expect(
    screen.getByRole("button", { name: "开始同传" }).closest("details"),
  ).toBe(disclosure);
  expect(screen.getByRole("tab", { name: "通话" }).closest("details")).toBe(
    disclosure,
  );
  await u.click(screen.getByText("语言与音频配置"));
  expect(screen.getByRole("button", { name: "开始同传" })).toBeDisabled();
  expect(screen.getByRole("tab", { name: "通话" })).toBeDisabled();
  expect(screen.getByRole("combobox", { name: "我的麦克风 A" })).toBeDisabled();
  expect(screen.getByRole("region", { name: "实时字幕" })).toBeInTheDocument();
});
it("shows supported language names in their native form", async () => {
  const port = workbenchDesktopFixture(),
    u = user();
  render(<App desktop={port} />);
  expect(screen.getByRole("combobox", { name: "原文语言" })).toHaveTextContent(
    "English",
  );
  await u.click(screen.getByRole("combobox", { name: "译文语言" }));
  expect(screen.getByRole("option", { name: "日本語" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "中文" })).toBeInTheDocument();
});
it("launches live input to M1 with the selected languages and locks configuration while Starting", async () => {
  const port = workbenchDesktopFixture(),
    u = user();
  render(<App desktop={port} />);
  await routes(u, false);
  await u.click(screen.getByRole("button", { name: "开始同传" }));
  await waitFor(() =>
    expect(port.lastArgs("create_session")).toEqual({
      launch: {
        config: {
          mode: "live",
          direction: "source_to_target",
          languages: { source: "en", target: "zh" },
        },
        provider_config_id: "00000000-0000-0000-0000-000000000001",
        devices: { mode: "live", input_id: "input", m1_output_id: "m1" },
      },
    }),
  );
  await u.click(screen.getByText("语言与音频配置"));
  expect(screen.getByRole("tab", { name: "通话" })).toBeDisabled();
  expect(screen.getByRole("combobox", { name: "音频输入" })).toBeDisabled();
  expect(screen.getByRole("combobox", { name: "同传服务" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "停止同传" })).toBeEnabled();
});

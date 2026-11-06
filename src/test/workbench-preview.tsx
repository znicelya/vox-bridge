/** Test-only browser entry. Never imported by src/main.tsx or production build. */
import ReactDOM from "react-dom/client";
import "@fontsource/ibm-plex-mono/latin-400.css";
import "@fontsource/ibm-plex-mono/latin-500.css";
import "@fontsource/ibm-plex-mono/latin-600.css";
import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-500.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@/index.css";
import App from "@/App";
import userEvent from "@testing-library/user-event";
import { within, waitFor } from "@testing-library/react";
import { workbenchDesktopFixture, workbenchWire } from "./workbench-fixture";
import wire from "../../tests/contracts/runtime-wire.json";
import {
  decodeTranscriptUpdate,
  decodeRuntimeDiagnostic,
  decodeSessionLaunch,
} from "@/lib/wire";
const scenario =
  new URLSearchParams(window.location.search).get("state") ?? "call";
const port = workbenchDesktopFixture();
if (scenario === "unavailable") port.rejectNext("list_audio_endpoints");
window.location.hash =
  scenario === "provider" || scenario === "audio" || scenario === "driver"
    ? "#settings"
    : "#workbench";
ReactDOM.createRoot(document.getElementById("root")!).render(
  <App desktop={port} />,
);
const banner = document.createElement("div");
banner.textContent = "离线界面测试 · 无真实音频或服务";
banner.setAttribute("aria-label", "离线界面测试");
banner.style.cssText =
  "position:fixed;right:16px;top:14px;font-size:12px;color:var(--muted-foreground);background:var(--background);padding-inline:8px";
document.body.appendChild(banner);
async function prepare() {
  const s = within(document.body),
    u = userEvent.setup({ delay: null });
  async function clickEnabled(name: string) {
    const button = await s.findByRole("button", { name });
    await waitFor(() => {
      if (button.hasAttribute("disabled"))
        throw Error("fixture control still pending");
    });
    await u.click(button);
  }
  async function choose(name: string, option: string) {
    const trigger = await s.findByRole("combobox", { name });
    await waitFor(
      () => {
        if (trigger.hasAttribute("disabled"))
          throw Error("fixture control still loading");
      },
      { timeout: 5000 },
    );
    await u.click(trigger);
    await u.click(await s.findByRole("option", { name: option }));
  }
  if (scenario === "provider") {
    await u.click(await s.findByRole("tab", { name: "同传服务" }));
    await choose("已保存配置", "测试同传服务");
    return;
  }
  if (scenario === "audio") return;
  if (scenario === "driver") {
    await clickEnabled("卸载驱动");
    return;
  }
  if (scenario === "unavailable") return;
  if (scenario === "call")
    await u.click(await s.findByRole("tab", { name: "通话" }));
  await choose("同传服务", "测试同传服务");
  await choose(scenario === "call" ? "我的麦克风 A" : "音频输入", "Microphone");
  await choose("M1 · VoxBridge Input", "VoxBridge Input");
  if (scenario === "call") {
    await choose("M2 · VoxBridge Output", "VoxBridge Output");
    await choose("我的扬声器", "Speakers");
  }
  await clickEnabled("开始同传");
  const original = workbenchWire.callRecord;
  const launch = decodeSessionLaunch(port.lastArgs("create_session")!.launch);
  port.setSession({
    ...original,
    mode: launch.config.mode,
    direction: launch.config.direction,
    context: { ...original.context!, launch },
    runtime: {
      phase: "running",
      outcome: null,
      directions:
        scenario === "call"
          ? [
              {
                direction: "source_to_target",
                phase: "running",
                failure: null,
                stream_epoch: 1,
              },
              {
                direction: "target_to_source",
                phase: "paused",
                failure: "provider",
                stream_epoch: 1,
              },
            ]
          : [
              {
                direction: "source_to_target",
                phase: "running",
                failure: null,
                stream_epoch: 1,
              },
            ],
    },
    status: "running",
  });
  const sourceRow =
    scenario === "call"
      ? wire.reverseSourceFinal
      : {
          ...wire.reverseSourceFinal,
          direction: "source_to_target",
          language: "en",
          text: "pump",
        };
  const translationRow =
    scenario === "call"
      ? wire.reverseTranslationFinal
      : {
          ...wire.reverseTranslationFinal,
          direction: "source_to_target",
          language: "zh",
          text: "泵",
        };
  for (const row of [sourceRow, translationRow])
    port.persistFinal(decodeTranscriptUpdate(row));
  for (const row of wire.diagnostics)
    port.persistDiagnostic(
      decodeRuntimeDiagnostic(
        scenario === "call" || row.direction === null
          ? row
          : {
              ...row,
              direction: "source_to_target",
            },
      ),
    );
  if (scenario === "scroll") {
    for (let index = 0; index < 80; index++)
      port.persistFinal(
        decodeTranscriptUpdate({
          ...sourceRow,
          id: `00000000-0000-0000-0000-${(700 + index).toString().padStart(12, "0")}`,
          sequence: 700 + index,
          text: `离线滚动字幕 ${index}。保留阅读位置并按需跳到最新字幕。`,
        }),
      );
    const append = document.createElement("button");
    append.textContent = "追加离线字幕";
    append.style.cssText =
      "margin-inline-start:12px;border:1px solid var(--border);padding:4px 8px";
    let next = 900;
    append.onclick = () => {
      const row = decodeTranscriptUpdate({
        ...sourceRow,
        id: `00000000-0000-0000-0000-${(next++).toString().padStart(12, "0")}`,
        sequence: next,
        text: "新增离线字幕。",
      });
      port.persistFinal(row);
      port.emit({ type: "transcript", update: row });
    };
    banner.appendChild(append);
  }
  port.emit({
    type: "resync_needed",
    session_id: original.id,
    missed_events: 0,
  });
}
void prepare();

import "@/test/workbench-dom";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
  act,
  fireEvent,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import App from "@/App";
import { workbenchDesktopFixture } from "@/test/workbench-fixture";
beforeEach(() => {
  window.location.hash = "#settings";
});
afterEach(cleanup);
async function choose(
  u: ReturnType<typeof userEvent.setup>,
  name: string,
  option: string,
) {
  await u.click(await screen.findByRole("combobox", { name }));
  await u.click(await screen.findByRole("option", { name: option }));
}
it("shows five settings destinations and requires a named device confirmation before uninstall", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  for (const name of ["音频", "同传服务", "模型", "客户研究", "本地数据"])
    expect(screen.getByRole("tab", { name })).toBeInTheDocument();
  await u.click(await screen.findByRole("button", { name: "卸载驱动" }));
  expect(port.lastArgs("manage_audio_driver")).toBeUndefined();
  const dialog = screen.getByRole("alertdialog");
  expect(dialog).toHaveTextContent("VoxBridge Input");
  expect(dialog).toHaveTextContent("VoxBridge Output");
  await u.click(within(dialog).getByRole("button", { name: "确认卸载" }));
  await waitFor(() =>
    expect(port.lastArgs("manage_audio_driver")).toEqual({
      operation: "uninstall",
    }),
  );
});
it("saves a write-only replacement once, clears the input, then explicitly keeps and clears credentials", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  await u.click(screen.getByRole("tab", { name: "同传服务" }));
  await choose(u, "已保存配置", "测试同传服务");
  const key = screen.getByLabelText("服务密钥");
  expect(key).toHaveValue("");
  expect(screen.getByText("密钥已配置")).toBeInTheDocument();
  await choose(u, "密钥操作", "替换密钥");
  await u.type(key, "synthetic-input-only");
  await u.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() => expect(key).toHaveValue(""));
  expect(
    (port.lastArgs("save_provider_config")!.request as { credential: unknown })
      .credential,
  ).toEqual({ action: "replace", api_key: "synthetic-input-only" });
  await u.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() =>
    expect(
      (
        port.lastArgs("save_provider_config")!.request as {
          credential: unknown;
        }
      ).credential,
    ).toEqual({ action: "keep" }),
  );
  await choose(u, "密钥操作", "清除密钥");
  await u.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() =>
    expect(screen.getByText("密钥未配置")).toBeInTheDocument(),
  );
  expect(
    (port.lastArgs("save_provider_config")!.request as { credential: unknown })
      .credential,
  ).toEqual({ action: "clear" });
});
it("destroys typed keys on navigation and confirms deletion of a saved profile", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  await u.click(screen.getByRole("tab", { name: "同传服务" }));
  await choose(u, "已保存配置", "测试同传服务");
  await choose(u, "密钥操作", "替换密钥");
  await u.type(screen.getByLabelText("服务密钥"), "temporary-never-saved");
  await u.click(screen.getByRole("link", { name: "实时工作台" }));
  await u.click(screen.getByRole("link", { name: "设置" }));
  await u.click(screen.getByRole("tab", { name: "同传服务" }));
  expect(screen.getByLabelText("服务密钥")).toHaveValue("");
  await choose(u, "已保存配置", "测试同传服务");
  await u.click(screen.getByRole("button", { name: "删除配置" }));
  expect(port.lastArgs("delete_provider_config")).toBeUndefined();
  await u.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "确认删除",
    }),
  );
  await waitFor(() =>
    expect(port.lastArgs("delete_provider_config")).toEqual({
      configId: "00000000-0000-0000-0000-000000000001",
    }),
  );
});
it("labels local preflight truthfully and never sends a connection probe", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  await u.click(screen.getByRole("tab", { name: "同传服务" }));
  await choose(u, "已保存配置", "测试同传服务");
  await u.click(screen.getByRole("button", { name: "检查配置" }));
  expect(await screen.findByText(/本地配置检查通过/)).toBeInTheDocument();
  expect(
    port.calls.some((x) => /probe|connect|test_provider/.test(x.command)),
  ).toBe(false);
});
it("clears a typed key on late native save success and prevents another save after local deadline", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  await u.click(screen.getByRole("tab", { name: "同传服务" }));
  await choose(u, "已保存配置", "测试同传服务");
  await choose(u, "密钥操作", "替换密钥");
  const key = screen.getByLabelText("服务密钥");
  await u.type(key, "synthetic-late-input");
  const gate = port.deferNext("save_provider_config");
  vi.useFakeTimers();
  fireEvent.submit(key.closest("form")!);
  await gate.requested;
  await act(async () => {
    await vi.advanceTimersByTimeAsync(10001);
  });
  vi.useRealTimers();
  expect(screen.getByText("桌面响应超时，请重试。")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "正在保存" })).toBeDisabled();
  gate.release();
  await waitFor(() => expect(key).toHaveValue(""));
  expect(
    port.calls.filter((x) => x.command === "save_provider_config"),
  ).toHaveLength(1);
});
it("treats uninstall Ready as completion and requires an explicit fresh endpoint check", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  await waitFor(() => expect(screen.getByText("驱动就绪")).toBeInTheDocument());
  port.replyNext("manage_audio_driver", "Ready");
  await u.click(screen.getByRole("button", { name: "卸载驱动" }));
  await u.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "确认卸载",
    }),
  );
  expect(
    await screen.findByText("卸载操作已完成，设备状态尚未重新检查。"),
  ).toBeInTheDocument();
  expect(screen.queryByText("驱动就绪")).not.toBeInTheDocument();
  expect(
    port.calls.filter((x) => x.command === "list_audio_endpoints"),
  ).toHaveLength(1);
  port.setEndpoints([]);
  await u.click(screen.getByRole("button", { name: "重新检查" }));
  expect(await screen.findByText("未检测到虚拟音频端点")).toBeInTheDocument();
  expect(screen.queryByText("驱动就绪")).not.toBeInTheDocument();
  expect(screen.queryByText("Microphone")).not.toBeInTheDocument();
});
it("marks the failing field for local validation and creates a new saved profile with a fresh ID", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  await u.click(screen.getByRole("tab", { name: "同传服务" }));
  await u.click(screen.getByRole("button", { name: "检查配置" }));
  expect(screen.getByLabelText("显示名称")).toHaveAttribute(
    "aria-invalid",
    "true",
  );
  expect(port.lastArgs("save_provider_config")).toBeUndefined();
  await u.type(screen.getByLabelText("显示名称"), "新增同传");
  await choose(u, "密钥操作", "替换密钥");
  await u.type(screen.getByLabelText("服务密钥"), "synthetic-new-profile");
  await u.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() =>
    expect(screen.getByText("配置已保存")).toBeInTheDocument(),
  );
  const request = port.lastArgs("save_provider_config")!.request as {
    config: { id: string; display_name: string };
    credential: unknown;
  };
  expect(request.config.id).not.toBe("00000000-0000-0000-0000-000000000001");
  expect(request.config.display_name).toBe("新增同传");
  expect(request.credential).toEqual({
    action: "replace",
    api_key: "synthetic-new-profile",
  });
  expect(screen.getByLabelText("服务密钥")).toHaveValue("");
});
it("keeps a failed profile deletion visible inside its confirmation and hides raw backend details", async () => {
  const port = workbenchDesktopFixture(),
    u = userEvent.setup();
  render(<App desktop={port} />);
  await u.click(screen.getByRole("tab", { name: "同传服务" }));
  await choose(u, "已保存配置", "测试同传服务");
  await u.click(screen.getByRole("button", { name: "删除配置" }));
  port.rejectNext(
    "delete_provider_config",
    "[CONFIGURATION_ERROR] private driver output",
  );
  await u.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "确认删除",
    }),
  );
  expect(
    await within(screen.getByRole("alertdialog")).findByRole("alert"),
  ).toHaveTextContent("会话配置不可用，请检查设置。");
  expect(screen.queryByText(/private driver output/)).not.toBeInTheDocument();
});

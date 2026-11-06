import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WireDecodeError, type Decoder } from "./wire";

export interface DesktopPort {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
}
/** The only Tauri transport import site; tests supply the same protocol port. */
export const desktopPort: DesktopPort = {
  invoke: (command, args) => invoke(command, args),
  listen<T>(event: string, handler: (payload: T) => void) {
    return listen<T>(event, (event) => handler(event.payload));
  },
};
class DesktopTimeoutError extends Error {}
export function withDesktopDeadline<T>(operation: Promise<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new DesktopTimeoutError()), 10000);
    operation.then(resolve, reject).finally(() => clearTimeout(timer));
  });
}
export function localDesktopError(error: unknown): string {
  if (error instanceof WireDecodeError) return "桌面数据格式无效，请重试。";
  if (error instanceof DesktopTimeoutError) return "桌面响应超时，请重试。";
  if (typeof error === "string") {
    const code = /^\[([A-Z_]+)\]/.exec(error)?.[1];
    if (code === "SESSION_NOT_FOUND") return "未找到此会话。";
    if (code === "STORAGE_ERROR") return "无法读取本地会话，请重试。";
    if (code === "CONFIGURATION_ERROR") return "会话配置不可用，请检查设置。";
    if (code === "AUDIO_ERROR") return "音频设备不可用，请检查设备。";
    if (code === "INVALID_REQUEST") return "会话请求无效，请重试。";
  }
  return "无法连接本地会话，请重试。";
}
export async function invokeDecoded<T>(
  port: DesktopPort,
  command: string,
  args: Record<string, unknown>,
  decode: Decoder<T>,
): Promise<T> {
  return decode(await withDesktopDeadline(port.invoke<unknown>(command, args)));
}

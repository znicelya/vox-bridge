import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "@/components/ui/alert-dialog";
import { localDesktopError } from "@/lib/desktop";
import { type DriverStatus } from "@/lib/wire";
import type { Workbench } from "@/features/workbench/useWorkbench";
export function driverLabel(s: DriverStatus | null) {
  if (s === null) return "状态未知";
  if (s === "Ready") return "驱动就绪";
  if (s === "NeedsRestart") return "需要重启";
  if (s === "NeedsPrivileges") return "需要系统授权";
  if ("NeedsApproval" in s) return "需要在系统设置批准驱动";
  return "驱动不可用，请检查平台安装说明。";
}
export function AudioSettings({ owner }: { owner: Workbench }) {
  const [confirm, setConfirm] = useState(false),
    [checking, setChecking] = useState(false),
    [error, setError] = useState<string | null>(null);
  async function manage(operation: "install" | "repair" | "uninstall") {
    setError(null);
    try {
      await owner.manageDriver(operation);
      setConfirm(false);
    } catch (e) {
      setError(localDesktopError(e));
    }
  }
  async function check() {
    setChecking(true);
    try {
      await owner.reload({ fresh: true, recheck: true });
    } catch (e) {
      setError(localDesktopError(e));
    } finally {
      setChecking(false);
    }
  }
  return (
    <div className="settings-body">
      <section className="settings-section">
        <div>
          <h2>逻辑音频路由</h2>
          <p>
            直播译声送入
            M1；通话的两个方向独立工作。请在通话软件中选择虚拟端点。
          </p>
        </div>
        <div className="settings-panel">
          <dl>
            <div>
              <dt>M1 · 虚拟麦克风</dt>
              <dd>VoxBridge Input</dd>
            </div>
            <div>
              <dt>M2 · 虚拟扬声器</dt>
              <dd>VoxBridge Output</dd>
            </div>
          </dl>
          {owner.inventory?.endpoints.map((x) => (
            <div className="endpoint-row" key={`${x.role}:${x.endpoint.id}`}>
              <span>{x.endpoint.display_name}</span>
              <span>{x.endpoint.ready ? "可用" : "不可用"}</span>
              <code>{x.endpoint.id}</code>
            </div>
          ))}
          {!owner.inventory && <p>尚未读取到本机音频设备。</p>}
        </div>
      </section>
      <section className="settings-section">
        <div>
          <h2>虚拟音频驱动</h2>
          <p>仅在您执行安装或修复时请求系统授权。应用不会修改系统默认设备。</p>
        </div>
        <div className="settings-panel">
          <p role="status">
            {owner.maintenance
              ? owner.maintenance.status === "Ready"
                ? `${{ install: "安装", repair: "修复", uninstall: "卸载" }[owner.maintenance.operation]}操作已完成，设备状态尚未重新检查。`
                : `${{ install: "安装", repair: "修复", uninstall: "卸载" }[owner.maintenance.operation]}操作结果：${driverLabel(owner.maintenance.status)}`
              : owner.inventory &&
                  !["M1Render", "M2Capture"].every((role) =>
                    owner.inventory!.endpoints.some((x) => x.role === role),
                  )
                ? "未检测到虚拟音频端点"
                : driverLabel(owner.driver)}
          </p>
          <div className="settings-actions">
            <Button
              variant="outline"
              disabled={checking || owner.configBusy || owner.loading}
              onClick={() => void check()}
            >
              {checking ? "正在检查" : "重新检查"}
            </Button>
            <Button
              variant="outline"
              disabled={owner.locked || owner.loading || !owner.inventory}
              onClick={() => void manage("install")}
            >
              安装驱动
            </Button>
            <Button
              variant="outline"
              disabled={owner.locked || owner.loading || !owner.inventory}
              onClick={() => void manage("repair")}
            >
              修复驱动
            </Button>
            <Button
              variant="outline"
              disabled={owner.locked || owner.loading || !owner.inventory}
              onClick={() => {
                setError(null);
                setConfirm(true);
              }}
            >
              卸载驱动
            </Button>
          </div>
          {owner.locked && <p>停止同传并等待音频释放后，才能更改驱动。</p>}
          {error && (
            <Alert>
              <AlertTitle>驱动操作未完成</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <details>
            <summary>平台安装说明</summary>
            <p>
              Windows 10 22H2 及以上：使用已签名的安装包，并按系统提示授权。
            </p>
            <p>macOS 13 Ventura 及以上：按系统设置提示批准驱动；需要时重启。</p>
            <p>
              Ubuntu 22.04 LTS 及以上：检查 PipeWire 或 PulseAudio
              可用，再重新检查端点。
            </p>
          </details>
        </div>
      </section>
      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>卸载虚拟音频驱动？</AlertDialogTitle>
            <AlertDialogDescription>
              将移除 VoxBridge Input（M1）与 VoxBridge
              Output（M2），依赖这些设备的通话软件将无法继续使用它们。
            </AlertDialogDescription>
          </AlertDialogHeader>
          {error && (
            <Alert>
              <AlertTitle>卸载未完成</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={owner.configBusy}>
              取消
            </AlertDialogCancel>
            <AlertDialogAction
              variant="outline"
              disabled={owner.locked}
              onClick={(e) => {
                e.preventDefault();
                void manage("uninstall");
              }}
            >
              确认卸载
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

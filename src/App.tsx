import "./App.css";
import { AppShell } from "@/features/shell/AppShell";
import { LiveWorkbench } from "@/features/workbench/LiveWorkbench";
import { SettingsPage } from "@/features/settings/SettingsPage";
import {
  useWorkbench,
  sessionStatusLabel,
} from "@/features/workbench/useWorkbench";
import { Button } from "@/components/ui/button";
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert";
import type { DesktopPort } from "@/lib/desktop";
function App({ desktop }: { desktop?: DesktopPort }) {
  const owner = useWorkbench(desktop);
  return (
    <AppShell
      persistentContent={
        <>
          {(owner.pending ||
            (owner.session &&
              !["failed", "completed"].includes(owner.session.status)) ||
            owner.stopping) && (
            <div className="active-session-bar">
              <span role="status">
                {owner.stopping
                  ? "正在停止"
                  : owner.pending
                    ? "正在启动同传"
                    : `同传 · ${sessionStatusLabel(owner.session?.status ?? "")}`}
              </span>
              <Button
                variant="outline"
                disabled={owner.stopping || owner.stopRequested}
                onClick={owner.stop}
              >
                {owner.stopRequested ? "已请求停止" : "停止同传"}
              </Button>
            </div>
          )}
        </>
      }
      renderContent={(route) =>
        route === "knowledge" || route === "history" ? null : (
          <div className="feature-route">
            {(owner.setupError || owner.error || owner.stream.error) && (
              <Alert>
                <AlertTitle>本机会话提示</AlertTitle>
                <AlertDescription>
                  {owner.error ?? owner.stream.error ?? owner.setupError}
                </AlertDescription>
                {owner.setupError && (
                  <Button
                    variant="outline"
                    onClick={() => void owner.reload()}
                    disabled={owner.loading}
                  >
                    重新读取配置
                  </Button>
                )}
              </Alert>
            )}
            {route === "workbench" ? (
              <LiveWorkbench owner={owner} />
            ) : route === "settings" ? (
              <SettingsPage owner={owner} />
            ) : null}
          </div>
        )
      }
    />
  );
}
export default App;

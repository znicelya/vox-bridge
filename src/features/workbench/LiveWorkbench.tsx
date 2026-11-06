import { LaunchControls } from "./LaunchControls";
import { AudioRoutePanel } from "./AudioRoutePanel";
import { ConversationStream } from "./ConversationStream";
import { ToolRail } from "./ToolRail";
import type { Workbench } from "./useWorkbench";
export function LiveWorkbench({ owner }: { owner: Workbench }) {
  return (
    <div className="live-workbench">
      <div className="workbench-primary">
        <LaunchControls owner={owner} />
        <AudioRoutePanel owner={owner} />
        <ConversationStream owner={owner} />
      </div>
      <ToolRail selection={owner.selection} />
    </div>
  );
}

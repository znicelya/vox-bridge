import { LaunchControls } from "./LaunchControls";
import { AudioRoutePanel } from "./AudioRoutePanel";
import { ConversationStream } from "./ConversationStream";
import { ToolRail, ToolSheet } from "./ToolRail";
import { isTerminal, type Workbench } from "./useWorkbench";
export function LiveWorkbench({ owner }: { owner: Workbench }) {
  return (
    <div
      className="live-workbench"
      data-active={
        owner.pending ||
        owner.stopping ||
        !!(owner.session && !isTerminal(owner.session))
      }
    >
      <div className="workbench-primary">
        <div className="workbench-toolbar">
          <LaunchControls owner={owner} />
          <ToolSheet selection={owner.selection} />
        </div>
        <AudioRoutePanel owner={owner} />
        <ConversationStream owner={owner} />
      </div>
      <ToolRail selection={owner.selection} />
    </div>
  );
}

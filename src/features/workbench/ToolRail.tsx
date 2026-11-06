import { BookOpenTextIcon, SearchIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyHeader,
  EmptyTitle,
  EmptyDescription,
  EmptyContent,
} from "@/components/ui/empty";
import {
  Sheet,
  SheetTrigger,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from "@/components/ui/sheet";
import type { SelectionContext } from "./useWorkbench";
function Tools({ selection }: { selection: SelectionContext | null }) {
  return (
    <div className="tool-content">
      {selection && (
        <section aria-label="检索上下文">
          <h2>所选内容</h2>
          <p className="selected-text">{selection.text}</p>
          <p className="workbench-note">
            {selection.segmentIds.length} 段 · 仅本地选择
          </p>
        </section>
      )}
      <Empty>
        <EmptyHeader>
          <EmptyTitle>本地知识</EmptyTitle>
          <EmptyDescription>
            配置嵌入模型并导入文档后，可从字幕查找本地资料。
          </EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <Button variant="outline" disabled>
            <BookOpenTextIcon data-icon="inline-start" />
            检索知识库
          </Button>
        </EmptyContent>
      </Empty>
      <Empty>
        <EmptyHeader>
          <EmptyTitle>客户研究</EmptyTitle>
          <EmptyDescription>
            配置 Tavily 后，可根据选中的内容发起客户研究。
          </EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <Button variant="outline" disabled>
            <SearchIcon data-icon="inline-start" />
            研究客户
          </Button>
        </EmptyContent>
      </Empty>
    </div>
  );
}
export function ToolRail({
  selection,
}: {
  selection: SelectionContext | null;
}) {
  return (
    <>
      <aside className="tool-rail" aria-label="知识与研究工具">
        <Tools selection={selection} />
      </aside>
      <div className="tool-mobile">
        <Sheet>
          <SheetTrigger asChild>
            <Button variant="outline">
              知识与研究{selection ? " · 已选择" : ""}
            </Button>
          </SheetTrigger>
          <SheetContent>
            <SheetHeader>
              <SheetTitle>知识与研究</SheetTitle>
              <SheetDescription>
                选择字幕后，使用已配置的服务。
              </SheetDescription>
            </SheetHeader>
            <Tools selection={selection} />
          </SheetContent>
        </Sheet>
      </div>
    </>
  );
}

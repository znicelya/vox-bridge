import { languageLabel } from "./languages";
import { useRef, useState } from "react";
import { ArrowDownIcon } from "lucide-react";
import {
  MessageScrollerProvider,
  MessageScroller,
  MessageScrollerViewport,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerButton,
} from "@/components/ui/message-scroller";
import {
  Message,
  MessageContent,
  MessageHeader,
  MessageFooter,
} from "@/components/ui/message";
import { Bubble, BubbleContent } from "@/components/ui/bubble";
import { Marker, MarkerContent } from "@/components/ui/marker";
import {
  Empty,
  EmptyHeader,
  EmptyTitle,
  EmptyDescription,
} from "@/components/ui/empty";
import { Button } from "@/components/ui/button";
import {
  sessionStatusLabel,
  type Workbench,
  type SelectionContext,
} from "./useWorkbench";
export function directionLabel(direction: string) {
  return direction === "source_to_target"
    ? "我 → 对方"
    : direction === "target_to_source"
      ? "对方 → 我"
      : "历史方向";
}
function timestamp(ms: number) {
  const date = new Date(ms);
  return Number.isFinite(date.valueOf())
    ? date.toLocaleTimeString("zh-CN", { hour12: false })
    : "—";
}
export function ConversationStream({ owner }: { owner: Workbench }) {
  const rows = [...owner.stream.finals, ...owner.stream.interims];
  const region = useRef<HTMLDivElement>(null),
    [partial, setPartial] = useState<SelectionContext | null>(null);
  function captureSelection() {
    const selected = window.getSelection();
    if (!selected || selected.isCollapsed || !region.current || !owner.session)
      return setPartial(null);
    const ids = rows
      .filter((row) => {
        const element = region.current!.querySelector(
          `[data-segment-id="${row.id}"] [data-transcript-text]`,
        );
        return (
          element &&
          Array.from({ length: selected.rangeCount }, (_, i) =>
            selected.getRangeAt(i),
          ).some((range) => range.intersectsNode(element))
        );
      })
      .map((x) => x.id);
    if (
      !ids.length ||
      !region.current.contains(selected.anchorNode) ||
      !region.current.contains(selected.focusNode)
    )
      return setPartial(null);
    const exact = selected.toString();
    if (exact.length > 262144) return setPartial(null);
    setPartial({ sessionId: owner.session.id, segmentIds: ids, text: exact });
  }
  const lastFinal = owner.stream.finals[owner.stream.finals.length - 1];
  return (
    <section className="conversation-panel" aria-label="实时字幕">
      <div className="conversation-heading">
        <h2>会话字幕</h2>
        {partial && (
          <Button variant="outline" onClick={() => owner.setSelection(partial)}>
            检索所选内容
          </Button>
        )}
      </div>
      <div
        className="sr-only"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        {owner.session
          ? `会话${sessionStatusLabel(owner.session.status)}`
          : "尚未开始"}
        {lastFinal
          ? `，新增${lastFinal.details?.kind === "translation" ? "译文" : "文本"}：${lastFinal.text.slice(0, 200)}`
          : ""}
      </div>
      <MessageScrollerProvider autoScroll defaultScrollPosition="end">
        <MessageScroller className="conversation-scroller">
          <MessageScrollerViewport
            aria-label="会话字幕滚动区域"
            ref={region}
            onMouseUp={captureSelection}
            onKeyUp={captureSelection}
          >
            <MessageScrollerContent aria-live="off">
              {owner.stream.hasEarlierRows && (
                <MessageScrollerItem messageId="earlier-rows">
                  <Marker>
                    <MarkerContent>
                      仅显示最近字幕，更早内容保留在本机会话。
                    </MarkerContent>
                  </Marker>
                </MessageScrollerItem>
              )}
              {rows.map((row) => (
                <MessageScrollerItem
                  key={row.id}
                  messageId={row.id}
                  data-segment-id={row.id}
                >
                  <Message align="start">
                    <MessageContent>
                      <MessageHeader>
                        <span>{directionLabel(row.direction)}</span>
                        <span>
                          {row.details?.kind === "source"
                            ? "原文"
                            : row.details?.kind === "translation"
                              ? "译文"
                              : "历史文本"}{" "}
                          · {languageLabel(row.language)}
                        </span>
                        <time
                          dateTime={
                            Number.isFinite(
                              new Date(row.occurred_at_ms).valueOf(),
                            )
                              ? new Date(row.occurred_at_ms).toISOString()
                              : undefined
                          }
                        >
                          {timestamp(row.occurred_at_ms)}
                        </time>
                        {!row.is_final && <span>识别中</span>}
                      </MessageHeader>
                      <Bubble variant="ghost">
                        <BubbleContent>
                          <p
                            data-transcript-text
                            className={
                              row.details?.kind === "translation"
                                ? "transcript-translation"
                                : "transcript-source"
                            }
                          >
                            {row.text}
                          </p>
                        </BubbleContent>
                      </Bubble>
                      <MessageFooter>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() =>
                            owner.setSelection({
                              sessionId: row.session_id,
                              segmentIds: [row.id],
                              text: row.text,
                            })
                          }
                        >
                          用此段检索
                        </Button>
                      </MessageFooter>
                    </MessageContent>
                  </Message>
                </MessageScrollerItem>
              ))}
            </MessageScrollerContent>
          </MessageScrollerViewport>
          <MessageScrollerButton direction="end" aria-label="跳到最新字幕">
            <ArrowDownIcon />
            <span className="sr-only">跳到最新字幕</span>
          </MessageScrollerButton>
        </MessageScroller>
      </MessageScrollerProvider>
      {rows.length === 0 && (
        <Empty className="conversation-empty">
          <EmptyHeader>
            <EmptyTitle>
              {owner.session ? "等待字幕" : "尚未开始本机会话"}
            </EmptyTitle>
            <EmptyDescription>
              {owner.session
                ? "服务返回的原文与译文将在这里分别显示。"
                : "选择语言、同传服务和音频路由后开始。"}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </section>
  );
}

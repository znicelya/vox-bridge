import "@/test/workbench-dom";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { AudioRoutePanel } from "./AudioRoutePanel";
import { ConversationStream } from "./ConversationStream";
import { workbenchWire } from "@/test/workbench-fixture";
import type { Workbench } from "./useWorkbench";
import { emptyStreamState } from "@/features/session-stream/reducer";
import { decodeRuntimeDiagnostic, decodeTranscriptUpdate } from "@/lib/wire";
afterEach(cleanup);
const base = {
  session: workbenchWire.callRecord,
  stream: emptyStreamState(),
  pending: false,
  stopping: false,
  resuming: [],
  resume: () => {},
  setSelection: () => {},
} as unknown as Workbench;
const diagnostic = (payload: unknown, index: number) =>
  decodeRuntimeDiagnostic({
    id: `00000000-0000-0000-0000-${index.toString().padStart(12, "0")}`,
    session_id: workbenchWire.callRecord.id,
    direction: "source_to_target",
    stream_epoch: 1,
    occurred_at_ms: 1730000000000,
    payload,
  });
it("keeps capture units distinct and playback counters in their native rate with unknown tail", () => {
  const owner = {
    ...base,
    stream: {
      ...base.stream,
      diagnostics: [
        diagnostic(
          {
            capture_accounting: {
              statistics: {
                dropped_native_blocks: 2,
                dropped_normalized_frames: 3,
                finalized: false,
              },
            },
          },
          100,
        ),
        diagnostic(
          {
            playback_accounting: {
              boundary: null,
              statistics: {
                input_sample_rate_hz: 24000,
                accepted_for_processing_samples: 3000,
                discarded_before_conversion_samples: 4,
                native: {
                  sample_rate_hz: 48000,
                  generated_mono_samples: 6000,
                  callback_consumed_mono_samples: 1000,
                  discarded_mono_samples: 2,
                },
                finalized: false,
                tail_disposition_known: false,
              },
            },
          },
          101,
        ),
        diagnostic(
          {
            latency: {
              capture_to_send_ms: 0,
              response_to_enqueue_ms: null,
              enqueue_to_callback_ms: 8,
              provider_processing_ms: null,
            },
          },
          102,
        ),
      ],
    },
  };
  render(<AudioRoutePanel owner={owner} />);
  expect(screen.getByText("播放尾部处置未知")).toBeInTheDocument();
  const capture = screen.getByText("丢弃原生音频块").closest("dl")!;
  expect(within(capture).getByText("2")).toBeInTheDocument();
  expect(within(capture).getByText("3")).toBeInTheDocument();
  expect(screen.getByText("24,000 Hz")).toBeInTheDocument();
  expect(screen.getByText("48,000 Hz")).toBeInTheDocument();
  expect(
    screen.getByText("采集到服务商队列").nextElementSibling,
  ).toHaveTextContent("0 ms");
  expect(
    screen.getByText("响应到播放队列").nextElementSibling,
  ).toHaveTextContent("—");
  expect(screen.queryByText(/已播放.*3000|2560|3840/)).not.toBeInTheDocument();
});
it("renders separate roles and unknown legacy time safely without interim live announcements", () => {
  const common = {
    session_id: workbenchWire.callRecord.id,
    direction: "source_to_target",
    sequence: 1,
    occurred_at_ms: Number.MAX_SAFE_INTEGER,
    language: "en",
    is_final: true,
    details: null,
  };
  const row = decodeTranscriptUpdate({
    ...common,
    id: "00000000-0000-0000-0000-000000000111",
    text: "Legacy exact text",
  });
  render(
    <ConversationStream
      owner={{ ...base, stream: { ...base.stream, finals: [row] } }}
    />,
  );
  expect(screen.getByText("历史文本 · English")).toBeInTheDocument();
  expect(screen.getByText("—")).toBeInTheDocument();
  expect(screen.getByRole("log")).toHaveAttribute("aria-live", "off");
  expect(screen.getByRole("status")).toHaveAttribute("aria-live", "polite");
});
it("replaces current unknown-tail notices with known final accounting and ignores old epochs", () => {
  const playback = (known: boolean, epoch: number, index: number) => ({
    ...diagnostic(
      {
        playback_accounting: {
          boundary: known ? "callback_consumed" : null,
          statistics: {
            input_sample_rate_hz: 24000,
            accepted_for_processing_samples: 0,
            discarded_before_conversion_samples: 0,
            native: null,
            finalized: known,
            tail_disposition_known: known,
          },
        },
      },
      index,
    ),
    stream_epoch: epoch,
  });
  const earlier = playback(false, 1, 120),
    final = playback(true, 1, 121);
  const view = render(
    <AudioRoutePanel
      owner={{
        ...base,
        stream: {
          ...base.stream,
          epochs: { source_to_target: 1 },
          diagnostics: [earlier],
        },
      }}
    />,
  );
  expect(screen.getByText("播放尾部处置未知")).toBeInTheDocument();
  view.rerender(
    <AudioRoutePanel
      owner={{
        ...base,
        stream: {
          ...base.stream,
          epochs: { source_to_target: 1 },
          diagnostics: [earlier, final],
        },
      }}
    />,
  );
  expect(screen.queryByText("播放尾部处置未知")).not.toBeInTheDocument();
  view.rerender(
    <AudioRoutePanel
      owner={{
        ...base,
        stream: {
          ...base.stream,
          epochs: { source_to_target: 2 },
          diagnostics: [earlier, final],
        },
      }}
    />,
  );
  expect(screen.queryByText("播放尾部处置未知")).not.toBeInTheDocument();
  view.rerender(
    <AudioRoutePanel
      owner={{
        ...base,
        stream: {
          ...base.stream,
          epochs: { source_to_target: 2 },
          diagnostics: [earlier, final, playback(false, 2, 122)],
        },
      }}
    />,
  );
  expect(screen.getByText("播放尾部处置未知")).toBeInTheDocument();
});

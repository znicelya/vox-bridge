import { Button } from "@/components/ui/button";
import { diagnosticLabel, type RuntimeDiagnostic } from "@/lib/wire";
import { directionLabel } from "./ConversationStream";
import type { Workbench } from "./useWorkbench";
const count = (v: number) => v.toLocaleString("en-US");
const phases: Record<string, string> = {
  starting: "启动中",
  running: "运行中",
  reconnecting: "重连中",
  paused: "已暂停",
  stopped: "已停止",
};
export const roleLabel = (role: string) =>
  ({
    Input: "麦克风输入",
    PhysicalOutput: "物理扬声器",
    M1Render: "M1 · VoxBridge Input",
    M2Capture: "M2 · VoxBridge Output",
  })[role] ?? "未知路由";
const boundary: Record<string, string> = {
  callback_consumed: "音频回调已消费",
  pulse_server_drained: "Pulse 服务端队列已排空",
  discarded: "已丢弃",
};
function Diagnostics({ diagnostic }: { diagnostic: RuntimeDiagnostic }) {
  const p = diagnostic.payload;
  // Only known decoded variants are interpreted; unknown payloads never echo raw fields.
  if ("latency" in p) {
    const x = p.latency as {
      capture_to_send_ms: number | null;
      response_to_enqueue_ms: number | null;
      enqueue_to_callback_ms: number | null;
      provider_processing_ms: number | null;
    };
    return (
      <dl>
        {Object.entries({
          采集到服务商队列: x.capture_to_send_ms,
          响应到播放队列: x.response_to_enqueue_ms,
          队列到音频回调: x.enqueue_to_callback_ms,
          服务商处理: x.provider_processing_ms,
        }).map(([name, v]) => (
          <div key={name}>
            <dt>{name}</dt>
            <dd>{v === null ? "—" : `${v} ms`}</dd>
          </div>
        ))}
      </dl>
    );
  }
  if ("capture_accounting" in p) {
    const x = (
      p.capture_accounting as {
        statistics: {
          dropped_native_blocks: number;
          dropped_normalized_frames: number;
          finalized: boolean;
        };
      }
    ).statistics;
    return (
      <dl>
        <div>
          <dt>丢弃原生音频块</dt>
          <dd>{count(x.dropped_native_blocks)}</dd>
        </div>
        <div>
          <dt>丢弃标准化帧</dt>
          <dd>{count(x.dropped_normalized_frames)}</dd>
        </div>
        <div>
          <dt>统计已定稿</dt>
          <dd>{x.finalized ? "是" : "否"}</dd>
        </div>
      </dl>
    );
  }
  if ("playback_accounting" in p) {
    const x = p.playback_accounting as {
        boundary: string | null;
        statistics: {
          input_sample_rate_hz: number;
          accepted_for_processing_samples: number;
          discarded_before_conversion_samples: number;
          native: {
            sample_rate_hz: number;
            generated_mono_samples: number;
            callback_consumed_mono_samples: number;
            discarded_mono_samples: number;
          } | null;
          finalized: boolean;
          tail_disposition_known: boolean;
        };
      },
      s = x.statistics;
    return (
      <dl>
        <div>
          <dt>输入采样率</dt>
          <dd>{count(s.input_sample_rate_hz)} Hz</dd>
        </div>
        <div>
          <dt>出队待处理输入样本</dt>
          <dd>{count(s.accepted_for_processing_samples)}</dd>
        </div>
        <div>
          <dt>转换前丢弃输入样本</dt>
          <dd>{count(s.discarded_before_conversion_samples)}</dd>
        </div>
        <div>
          <dt>原生单声道采样率</dt>
          <dd>{s.native ? `${count(s.native.sample_rate_hz)} Hz` : "—"}</dd>
        </div>
        {s.native &&
          Object.entries({
            生成原生单声道样本: s.native.generated_mono_samples,
            回调消费原生单声道样本: s.native.callback_consumed_mono_samples,
            丢弃原生单声道样本: s.native.discarded_mono_samples,
          }).map(([k, v]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>{count(v)}</dd>
            </div>
          ))}
        <div>
          <dt>播放边界</dt>
          <dd>{x.boundary ? boundary[x.boundary] : "—"}</dd>
        </div>
        <div>
          <dt>统计已定稿</dt>
          <dd>{s.finalized ? "是" : "否"}</dd>
        </div>
        <div>
          <dt>尾部处置已知</dt>
          <dd>{s.tail_disposition_known ? "是" : "否"}</dd>
        </div>
      </dl>
    );
  }
  if ("failure" in p) {
    const x = p.failure as {
      code: string;
      provider_code: number | null;
      log_id: string | null;
    };
    return (
      <dl>
        <div>
          <dt>错误代码</dt>
          <dd>{x.code}</dd>
        </div>
        <div>
          <dt>服务商代码</dt>
          <dd>{x.provider_code ?? "—"}</dd>
        </div>
        <div>
          <dt>logid</dt>
          <dd>{x.log_id ?? "—"}</dd>
        </div>
      </dl>
    );
  }
  if ("playback" in p) {
    const x = p.playback as { boundary: string };
    return <p>{boundary[x.boundary]}</p>;
  }
  return <p>{diagnosticLabel(diagnostic)}</p>;
}
export function AudioRoutePanel({ owner }: { owner: Workbench }) {
  if (!owner.session) return null;
  const accounting = new Map<string, RuntimeDiagnostic>();
  for (const diagnostic of owner.stream.diagnostics) {
    if (!("playback_accounting" in diagnostic.payload)) continue;
    const currentEpoch = diagnostic.direction
      ? owner.stream.epochs[diagnostic.direction]
      : undefined;
    if (currentEpoch !== undefined && diagnostic.stream_epoch !== currentEpoch)
      continue;
    accounting.set(diagnostic.direction ?? "session", diagnostic);
  }
  const unknownTail = [...accounting.values()].some(
    (diagnostic) =>
      "playback_accounting" in diagnostic.payload &&
      !(
        diagnostic.payload.playback_accounting as {
          statistics: { tail_disposition_known: boolean };
        }
      ).statistics.tail_disposition_known,
  );
  return (
    <section className="audio-status" aria-label="音频方向状态">
      <h2>音频路由</h2>
      {owner.session.runtime?.directions.map((d) => (
        <div key={d.direction} className="audio-direction">
          <span>{directionLabel(d.direction)}</span>
          <span>{phases[d.phase] ?? "—"}</span>
          {d.phase === "paused" && (
            <Button
              variant="outline"
              size="sm"
              disabled={
                owner.pending ||
                owner.stopping ||
                owner.resuming.includes(d.direction)
              }
              onClick={() => void owner.resume(d.direction)}
            >
              {d.direction === "target_to_source"
                ? "继续对方到我"
                : "继续我到对方"}
            </Button>
          )}
          {d.failure && (
            <span className="workbench-note">
              {(
                {
                  provider: "服务暂不可用",
                  device_unavailable: "设备已断开",
                  audio_backend_unavailable: "音频服务不可用",
                  storage: "本地保存失败",
                  startup_timeout: "启动超时",
                  finish_timeout: "结束超时",
                  playback_timeout: "播放超时",
                  cleanup_timeout: "释放音频超时",
                  interrupted: "会话已中断",
                  invalid_event: "服务响应不可用",
                } as Record<string, string>
              )[d.failure] ?? "方向已暂停"}
            </span>
          )}
        </div>
      ))}
      <details>
        <summary>
          <span>路由与测量详情</span>
          {unknownTail && (
            <span className="workbench-note">
              {" "}
              · <span>播放尾部处置未知</span>
            </span>
          )}
        </summary>
        <p className="session-id">会话 {owner.session.id}</p>
        {owner.session.context?.endpoints.map((x) => (
          <p key={`${x.role}:${x.endpoint.id}`}>
            {roleLabel(x.role)} · {x.endpoint.display_name}{" "}
            <span className="session-id">{x.endpoint.id}</span>
          </p>
        ))}
        {owner.stream.diagnostics.slice(-20).map((d) => (
          <section key={d.id}>
            <h3>
              {diagnosticLabel(d)} ·{" "}
              {d.direction ? directionLabel(d.direction) : "会话"}
            </h3>
            <Diagnostics diagnostic={d} />
          </section>
        ))}
      </details>
    </section>
  );
}

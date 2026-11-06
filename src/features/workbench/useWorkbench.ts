import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import {
  desktopPort,
  localDesktopError,
  withDesktopDeadline,
  type DesktopPort,
} from "@/lib/desktop";
import {
  decodeAudioInventory,
  decodeProviderConfigList,
  decodeProviderCapabilities,
  decodeLaunchValidation,
  decodeSessionRecord,
  decodeDriverStatus,
  type AudioInventory,
  type ProviderConfigView,
  type ProviderCapabilities,
  type SessionLaunch,
  type SessionRecord,
  type SessionDirection,
  type SessionMode,
  type DriverStatus,
} from "@/lib/wire";
import { createSessionStream } from "@/features/session-stream/controller";
export interface SelectionContext {
  sessionId: string;
  segmentIds: string[];
  text: string;
}
export const sessionStatusLabel = (status: string) =>
  (
    ({
      created: "已创建",
      starting: "启动中",
      running: "运行中",
      stopping: "停止中",
      paused: "已暂停",
      failed: "已结束，存在错误",
      completed: "已完成",
    }) as Record<string, string>
  )[status] ?? "状态未知";
export const isTerminal = (s: SessionRecord | null) =>
  s?.status === "completed" || s?.status === "failed";
export function useWorkbench(port: DesktopPort = desktopPort) {
  const [stream] = useState(() => createSessionStream(port));
  const view = useSyncExternalStore(stream.subscribe, stream.snapshot);
  const [inventory, setInventory] = useState<AudioInventory | null>(null),
    [profiles, setProfiles] = useState<ProviderConfigView[]>([]),
    [capabilities, setCapabilities] = useState<
      Record<string, ProviderCapabilities>
    >({});
  const [loading, setLoading] = useState(true),
    [setupError, setSetupError] = useState<string | null>(null),
    [error, setError] = useState<string | null>(null);
  const [record, setRecord] = useState<SessionRecord | null>(null),
    [pending, setPending] = useState(false),
    [stopRequested, setStopRequested] = useState(false),
    [stopping, setStopping] = useState(false),
    [resuming, setResuming] = useState<SessionDirection[]>([]);
  const [knownEndpoints, setKnownEndpoints] = useState<Record<string, string>>(
    {},
  );
  const [maintenance, setMaintenance] = useState<{
    operation: "install" | "repair" | "uninstall";
    status: DriverStatus;
  } | null>(null);
  const maintenanceRef = useRef(false);
  const [configBusy, setConfigBusy] = useState(false),
    [driver, setDriver] = useState<DriverStatus | null>(null),
    [selection, setSelection] = useState<SelectionContext | null>(null);
  const mounted = useRef(false),
    startWork = useRef<{
      stage: "validate" | "create";
      cancel: boolean;
    } | null>(null),
    stopWork = useRef<Promise<unknown> | null>(null),
    resumeWork = useRef(new Set<SessionDirection>()),
    configWork = useRef<Promise<unknown> | null>(null),
    recordRef = useRef<SessionRecord | null>(null),
    loadGeneration = useRef(0),
    reloadWork = useRef<Promise<void> | null>(null),
    reloadLocal = useRef<Promise<void> | null>(null),
    reloadAgain = useRef(false),
    recheckNeeded = useRef(false);
  const session = isTerminal(record)
    ? record
    : view.session?.id === record?.id
      ? view.session
      : record;
  const [draft, setDraft] = useState<{
    mode: SessionMode;
    source: string;
    target: string;
    provider: string;
    devices: Record<string, string>;
  }>({ mode: "live", source: "en", target: "zh", provider: "", devices: {} });
  recordRef.current = session;
  const locked =
    pending ||
    stopping ||
    resuming.length > 0 ||
    configBusy ||
    !!(session && !isTerminal(session));
  function report(e: unknown) {
    if (mounted.current) setError(localDesktopError(e));
  }
  // Native completion, rather than the UI deadline, releases command admission.
  function watch<T>(native: Promise<T>) {
    void withDesktopDeadline(native).catch(report);
    return native;
  }
  function reload(
    options: { fresh?: boolean; recheck?: boolean } = {},
  ): Promise<void> {
    if (reloadWork.current) {
      if (options.fresh || options.recheck) {
        ++loadGeneration.current;
        reloadAgain.current = true;
        recheckNeeded.current ||= !!options.recheck;
      }
      return reloadLocal.current!;
    }
    ++loadGeneration.current;
    recheckNeeded.current ||= !!options.recheck;
    setLoading(true);
    // One native read at a time, including capability reads. Local deadlines never
    // retire this owner; repeated rechecks share one local waiter and one latest refresh.
    const work = Promise.resolve()
      .then(async () => {
        do {
          reloadAgain.current = false;
          const generation = loadGeneration.current;
          const current = () =>
            mounted.current && generation === loadGeneration.current;
          let checkedDriver: DriverStatus | null = null;
          try {
            if (recheckNeeded.current) {
              recheckNeeded.current = false;
              checkedDriver = decodeDriverStatus(
                await port.invoke("audio_driver_status", {}),
              );
              if (!current()) continue;
            }
            const audio = decodeAudioInventory(
              await port.invoke("list_audio_endpoints", {}),
            );
            if (!current()) continue;
            const saved = decodeProviderConfigList(
              await port.invoke("list_provider_configs", {}),
            );
            if (!current()) continue;
            const caps: Record<string, ProviderCapabilities> = {};
            for (const profile of saved) {
              if (!current()) break;
              try {
                caps[profile.config.id] = decodeProviderCapabilities(
                  await port.invoke("provider_capabilities", {
                    configId: profile.config.id,
                  }),
                );
              } catch {
                /* Capability remains unknown and unavailable. */
              }
            }
            if (!current()) continue;
            if (checkedDriver !== null) {
              maintenanceRef.current = false;
              setMaintenance(null);
            }
            if (!maintenanceRef.current) setInventory(audio);
            setKnownEndpoints((old) => ({
              ...old,
              ...Object.fromEntries(
                audio.endpoints.map((x) => [
                  x.endpoint.id,
                  x.endpoint.display_name,
                ]),
              ),
            }));
            setDriver(checkedDriver ?? audio.driver);
            setProfiles(saved);
            setCapabilities(caps);
            setSetupError(null);
          } catch {
            if (current())
              setSetupError(
                "桌面服务不可用，请在桌面应用中检查音频与同传服务。",
              );
          }
        } while (mounted.current && reloadAgain.current);
      })
      .finally(() => {
        reloadWork.current = null;
        reloadLocal.current = null;
        if (mounted.current) setLoading(false);
      });
    reloadWork.current = work;
    const local = withDesktopDeadline(work).catch(() => {
      if (mounted.current) setSetupError("桌面响应超时，仍在等候本机配置。");
    });
    reloadLocal.current = local;
    return local;
  }
  useEffect(() => {
    mounted.current = true;
    void reload({ fresh: true });
    return () => {
      mounted.current = false;
      ++loadGeneration.current;
      void stream.stop();
    };
  }, [stream]);
  async function stopRecord(s: SessionRecord) {
    if (stopWork.current) return;
    setStopping(true);
    const native = port
      .invoke<unknown>("stop_session", { sessionId: s.id })
      .then(decodeSessionRecord);
    stopWork.current = native;
    watch(native);
    try {
      const ended = await native;
      if (mounted.current) {
        setRecord(ended);
        recordRef.current = ended;
      }
    } catch (e) {
      report(e);
    } finally {
      stopWork.current = null;
      if (mounted.current) {
        setStopping(false);
        setStopRequested(false);
      }
    }
  }
  async function start(launch: SessionLaunch) {
    if (
      startWork.current ||
      stopWork.current ||
      resumeWork.current.size ||
      configWork.current ||
      (recordRef.current && !isTerminal(recordRef.current))
    )
      return;
    const work: { stage: "validate" | "create"; cancel: boolean } = {
      stage: "validate",
      cancel: false,
    };
    startWork.current = work;
    setPending(true);
    setError(null);
    setStopRequested(false);
    setSelection(null);
    try {
      await watch(
        port
          .invoke<unknown>("validate_session_launch", { launch })
          .then(decodeLaunchValidation),
      );
      if (work.cancel) return;
      work.stage = "create";
      const created = await watch(
        port
          .invoke<unknown>("create_session", { launch })
          .then(decodeSessionRecord),
      );
      if (mounted.current) {
        recordRef.current = created;
        setRecord(created);
        void stream.start(created.id);
      }
      if (work.cancel) await stopRecord(created);
    } catch (e) {
      report(e);
    } finally {
      if (startWork.current === work) startWork.current = null;
      if (mounted.current) {
        setPending(false);
        if (!stopWork.current) setStopRequested(false);
      }
    }
  }
  function stop() {
    if (startWork.current) {
      startWork.current.cancel = true;
      setStopRequested(true);
      return;
    }
    if (recordRef.current && !isTerminal(recordRef.current))
      void stopRecord(recordRef.current);
  }
  async function resume(direction: SessionDirection) {
    const s = recordRef.current;
    if (
      !s ||
      startWork.current ||
      stopWork.current ||
      resumeWork.current.has(direction) ||
      !s.runtime?.directions.some(
        (x) => x.direction === direction && x.phase === "paused",
      )
    )
      return;
    resumeWork.current.add(direction);
    setResuming([...resumeWork.current]);
    setError(null);
    const native = port.invoke("resume_session_direction", {
      sessionId: s.id,
      direction,
    });
    watch(native);
    try {
      await native;
    } catch (e) {
      report(e);
    } finally {
      resumeWork.current.delete(direction);
      if (mounted.current) setResuming([...resumeWork.current]);
    }
  }
  async function configuration<T>(
    command: string,
    args: Record<string, unknown>,
    decode: (x: unknown) => T,
  ): Promise<T> {
    if (
      startWork.current ||
      stopWork.current ||
      resumeWork.current.size ||
      configWork.current ||
      (recordRef.current && !isTerminal(recordRef.current))
    )
      throw "[CONFIGURATION_ERROR] audio owned";
    setConfigBusy(true);
    const native = port.invoke<unknown>(command, args).then(decode);
    configWork.current = native;
    void native.then(
      () => {
        configWork.current = null;
        if (mounted.current) setConfigBusy(false);
      },
      () => {
        configWork.current = null;
        if (mounted.current) setConfigBusy(false);
      },
    );
    return watch(native);
  }
  async function manageDriver(operation: "install" | "repair" | "uninstall") {
    const status = await configuration(
      "manage_audio_driver",
      { operation },
      decodeDriverStatus,
    );
    maintenanceRef.current = true;
    ++loadGeneration.current;
    setInventory(null);
    setDriver(null);
    setMaintenance({ operation, status });
    setError(null);
    return status;
  }
  return {
    draft,
    setDraft,
    port,
    stream: view,
    session,
    inventory,
    knownEndpoints,
    maintenance,
    profiles,
    capabilities,
    loading,
    setupError,
    error,
    locked,
    pending,
    stopping,
    stopRequested,
    resuming,
    configBusy,
    driver,
    selection,
    setSelection,
    start,
    stop,
    resume,
    reload,
    configuration,
    manageDriver,
    setDriver,
  };
}
export type Workbench = ReturnType<typeof useWorkbench>;

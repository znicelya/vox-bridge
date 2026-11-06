/** Offline only. Not imported by the production entry. */
import wireJson from "../../tests/contracts/runtime-wire.json";
import {
  decodeSessionRecord,
  decodeSessionLaunch,
  type ProviderConfigView,
  type ProviderCapabilities,
  type SaveProviderRequest,
} from "@/lib/wire";
import { runtimeDesktopFixture } from "./desktop-fixture";
export const workbenchWire = {
  callRecord: decodeSessionRecord(wireJson.callRecord),
  callLaunch: decodeSessionLaunch(wireJson.callLaunch),
  terminalRecord: decodeSessionRecord(wireJson.terminalRecord),
};
export function workbenchDesktopFixture() {
  let endpoints = structuredClone(workbenchWire.callRecord.context!.endpoints);
  const profile: ProviderConfigView = {
    config: {
      id: workbenchWire.callLaunch.provider_config_id,
      provider: "volcengine",
      display_name: "测试同传服务",
      endpoint: "wss://openspeech.bytedance.com/api/v4/ast/v2/translate",
      llm: null,
      embedding: null,
      tavily: null,
      language_capabilities: [],
      secret_reference: null,
    },
    credential_configured: true,
  };
  let profiles = [profile];
  const capabilities: ProviderCapabilities = {
    language_pairs: [
      { source: "en", target: "zh" },
      { source: "zh", target: "en" },
    ],
    input_audio: { sample_rate: 16000, channels: 1, sample_format: "i16" },
    output_audio: { sample_rate: 24000, channels: 1, sample_format: "i16" },
    native_chunk_duration: { secs: 0, nanos: 80000000 },
    supports_tts: true,
    supports_bidirectional: true,
    ready: true,
    unavailable_reason: null,
  };
  const port = runtimeDesktopFixture(workbenchWire, (command, args) => {
    switch (command) {
      case "list_audio_endpoints":
        return {
          generation: "00000000-0000-0000-0000-000000000010",
          driver: "Ready",
          endpoints,
        };
      case "audio_driver_status":
        return "Ready";
      case "manage_audio_driver":
        return args.operation === "uninstall" ? "NeedsRestart" : "Ready";
      case "list_provider_configs":
        return profiles;
      case "provider_capabilities":
        return capabilities;
      case "validate_session_launch": {
        const launch = decodeSessionLaunch(args.launch);
        const ids = Object.entries(launch.devices)
          .filter(([k]) => k !== "mode")
          .map(([, id]) => id);
        if (
          ids.some(
            (id) =>
              !endpoints.some((x) => x.endpoint.id === id && x.endpoint.ready),
          )
        )
          throw "[AUDIO_ERROR] stale endpoint";
        return {
          context: { ...workbenchWire.callRecord.context, launch, endpoints },
          input_audio: capabilities.input_audio,
          output_audio: capabilities.output_audio,
        };
      }
      case "create_session": {
        const launch = decodeSessionLaunch(args.launch);
        const record = {
          ...workbenchWire.callRecord,
          mode: launch.config.mode,
          direction: launch.config.direction,
          runtime: {
            phase: "starting" as const,
            outcome: null,
            directions: (launch.config.mode === "call"
              ? (["source_to_target", "target_to_source"] as const)
              : (["source_to_target"] as const)
            ).map((direction) => ({
              direction,
              phase: "starting" as const,
              stream_epoch: 0,
              failure: null,
            })),
          },
          context: { ...workbenchWire.callRecord.context!, launch },
        };
        port.setSession(record);
        return record;
      }
      case "stop_session":
        port.setSession(workbenchWire.terminalRecord);
        return workbenchWire.terminalRecord;
      case "resume_session_direction":
        return null;
      case "save_provider_config": {
        const request = args.request as SaveProviderRequest;
        const old = profiles.find((x) => x.config.id === request.config.id);
        const saved = {
          config: {
            ...request.config,
            display_name:
              request.config.display_name?.trim() || request.config.provider,
          },
          credential_configured:
            request.credential.action === "keep"
              ? (old?.credential_configured ?? false)
              : request.credential.action === "replace",
        } as ProviderConfigView;
        profiles = [
          ...profiles.filter((x) => x.config.id !== saved.config.id),
          saved,
        ];
        return saved;
      }
      case "delete_provider_config":
        profiles = profiles.filter((x) => x.config.id !== args.configId);
        return null;
      default:
        throw `unsupported workbench fixture command ${command}`;
    }
  });
  return Object.assign(port, {
    setEndpoints(value: typeof endpoints) {
      endpoints = value;
    },
    setProfiles(value: typeof profiles) {
      profiles = value;
    },
    setCapabilities(value: Partial<typeof capabilities>) {
      Object.assign(capabilities, value);
    },
    lastArgs(command: string) {
      const found = port.calls.filter((x) => x.command === command);
      return found[found.length - 1]?.args;
    },
  });
}

import { languageLabel } from "./languages";
import { useState } from "react";
import {
  Field,
  FieldGroup,
  FieldLabel,
  FieldDescription,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import type { SessionMode, SessionLaunch } from "@/lib/wire";
import type { Workbench } from "./useWorkbench";
export function Choice({
  id,
  label,
  value,
  onChange,
  options,
  disabled = false,
  description,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string; disabled?: boolean }[];
  disabled?: boolean;
  description?: string;
}) {
  return (
    <Field data-disabled={disabled || undefined}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Select value={value} onValueChange={onChange} disabled={disabled}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue placeholder="请选择" />
        </SelectTrigger>
        <SelectContent position="popper">
          <SelectGroup>
            {options.map((x) => (
              <SelectItem key={x.value} value={x.value} disabled={x.disabled}>
                {x.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      {description && <FieldDescription>{description}</FieldDescription>}
    </Field>
  );
}
export function LaunchControls({ owner }: { owner: Workbench }) {
  const { mode, source, target, provider, devices } = owner.draft;
  const change = (
    key: "mode" | "source" | "target" | "provider",
    value: string,
  ) => {
    if (!owner.locked) owner.setDraft((old) => ({ ...old, [key]: value }));
  };
  const [preflight, setPreflight] = useState<string | null>(null);
  const languages = new Set(["en", "zh", "ja", "ko", "fr", "de", "es"]);
  for (const capability of Object.values(owner.capabilities))
    for (const pair of capability.language_pairs) {
      languages.add(pair.source);
      languages.add(pair.target);
    }
  const langs = [...languages].map((value) => ({
    value,
    label: languageLabel(value),
  }));
  const profile = owner.profiles.find((x) => x.config.id === provider),
    cap = owner.capabilities[provider];
  const routes =
    mode === "live"
      ? [
          ["input_id", "Input", "音频输入"],
          ["m1_output_id", "M1Render", "M1 · VoxBridge Input"],
        ]
      : [
          ["a_input_id", "Input", "我的麦克风 A"],
          ["m2_input_id", "M2Capture", "M2 · VoxBridge Output"],
          ["m1_output_id", "M1Render", "M1 · VoxBridge Input"],
          ["physical_output_id", "PhysicalOutput", "我的扬声器"],
        ];
  const complete =
    !!owner.inventory &&
    !!profile?.credential_configured &&
    !!cap?.ready &&
    routes.every(
      ([key, role]) =>
        !!devices[key] &&
        owner.inventory!.endpoints.some(
          (x) =>
            x.role === role &&
            x.endpoint.id === devices[key] &&
            x.endpoint.ready,
        ),
    );
  function launch() {
    if (owner.locked || !complete) return;
    if (
      !cap?.language_pairs.some(
        (x) => x.source === source && x.target === target,
      ) ||
      (mode === "call" &&
        (!cap.supports_bidirectional ||
          !cap.language_pairs.some(
            (x) => x.source === target && x.target === source,
          )))
    ) {
      setPreflight("此服务不支持所选语言方向，请调整语言。");
      return;
    }
    const selected = Object.fromEntries(
      routes.map(([key]) => [key, devices[key]]),
    );
    const launch = {
      config: {
        mode,
        direction: mode === "call" ? "bidirectional" : "source_to_target",
        languages: { source, target },
      },
      provider_config_id: provider,
      devices: { mode, ...selected },
    } as SessionLaunch;
    setPreflight(null);
    void owner.start(launch);
  }
  return (
    <section className="launch-panel" aria-label="同传配置">
      <details className="launch-disclosure" open={!owner.locked || undefined}>
        <summary>
          <span>语言与音频配置</span>
          {owner.locked && (
            <span className="launch-mode">
              {mode === "call" ? "通话" : "直播"}
            </span>
          )}
        </summary>
        <Tabs
          value={mode}
          onValueChange={(v) => {
            if (!owner.locked) change("mode", v as SessionMode);
          }}
        >
          <div className="workbench-actions">
            <TabsList variant="line">
              <TabsTrigger value="live" disabled={owner.locked}>
                直播
              </TabsTrigger>
              <TabsTrigger value="call" disabled={owner.locked}>
                通话
              </TabsTrigger>
            </TabsList>
            <Button onClick={launch} disabled={owner.locked || !complete}>
              {owner.pending ? "正在启动" : "开始同传"}
            </Button>
          </div>
          <TabsContent value={mode}>
            <FieldGroup className="launch-fields">
              <Choice
                id="source-language"
                label="原文语言"
                value={source}
                onChange={(v) => change("source", v)}
                options={langs}
                disabled={owner.locked}
              />
              <Choice
                id="target-language"
                label="译文语言"
                value={target}
                onChange={(v) => change("target", v)}
                options={langs}
                disabled={owner.locked}
              />
              <Choice
                id="provider-config"
                label="同传服务"
                value={provider}
                onChange={(v) => change("provider", v)}
                disabled={owner.locked || owner.loading}
                options={owner.profiles.map((p) => ({
                  value: p.config.id,
                  label: p.config.display_name,
                  disabled:
                    !owner.capabilities[p.config.id]?.ready ||
                    !p.credential_configured,
                }))}
              />
              {routes.map(([key, role, label]) => (
                <Choice
                  key={key}
                  id={key}
                  label={label}
                  value={devices[key] ?? ""}
                  onChange={(v) =>
                    owner.setDraft((old) => ({
                      ...old,
                      devices: { ...old.devices, [key]: v },
                    }))
                  }
                  disabled={owner.locked || !owner.inventory}
                  options={[
                    ...(owner.inventory?.endpoints ?? [])
                      .filter((x) => x.role === role)
                      .map((x) => ({
                        value: x.endpoint.id,
                        label: x.endpoint.display_name,
                        disabled: !x.endpoint.ready,
                      })),
                    ...(devices[key] &&
                    !owner.inventory?.endpoints.some(
                      (x) => x.role === role && x.endpoint.id === devices[key],
                    )
                      ? [
                          {
                            value: devices[key],
                            label: `${owner.knownEndpoints[devices[key]] ?? devices[key]}（${owner.inventory ? "不可用" : "待重新检查"}）`,
                            disabled: true,
                          },
                        ]
                      : []),
                  ]}
                />
              ))}
            </FieldGroup>
          </TabsContent>
        </Tabs>
        {owner.locked && (
          <p className="workbench-note">同传占用音频期间，停止后可修改配置。</p>
        )}
      </details>
      {preflight && <p role="alert">{preflight}</p>}
      {owner.loading && <p role="status">正在读取本机配置…</p>}
      {!owner.loading && owner.profiles.length === 0 && (
        <p>尚无同传服务，先到设置添加服务配置。</p>
      )}
      {profile && !cap?.ready && (
        <p>
          {cap?.unavailable_reason ? "未联调" : "能力未知"}，请检查服务设置。
        </p>
      )}
    </section>
  );
}

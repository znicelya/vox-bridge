import { useState } from "react";
import {
  Field,
  FieldGroup,
  FieldLabel,
  FieldDescription,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "@/components/ui/alert-dialog";
import { Choice } from "@/features/workbench/LaunchControls";
import { localDesktopError } from "@/lib/desktop";
import {
  decodeProviderConfigView,
  type ProviderConfigView,
  type CredentialChange,
  type PublicProviderConfig,
} from "@/lib/wire";
import type { Workbench } from "@/features/workbench/useWorkbench";
const endpoint = "wss://openspeech.bytedance.com/api/v4/ast/v2/translate";
const blank = (): PublicProviderConfig => ({
  id: crypto.randomUUID(),
  provider: "volcengine",
  display_name: "",
  endpoint,
  llm: null,
  embedding: null,
  tavily: null,
  language_capabilities: [],
  secret_reference: null,
});
export function ProviderSettings({ owner }: { owner: Workbench }) {
  const [config, setConfig] = useState(blank),
    [selected, setSelected] = useState(""),
    [configured, setConfigured] = useState(false),
    [key, setKey] = useState(""),
    [action, setAction] = useState<"keep" | "replace" | "clear">("keep"),
    [confirm, setConfirm] = useState(false),
    [error, setError] = useState<string | null>(null),
    [notice, setNotice] = useState<string | null>(null);
  // Secret is owned only by this mounted form. Selecting another profile destroys its value.
  function load(view?: ProviderConfigView) {
    setConfig(view?.config ?? blank());
    setSelected(view?.config.id ?? "");
    setConfigured(view?.credential_configured ?? false);
    setKey("");
    setAction("keep");
    setError(null);
    setNotice(null);
  }
  const cap = owner.capabilities[config.id];
  const disabled = owner.locked;
  const nameInvalid = !!error?.startsWith("显示名称"),
    endpointInvalid = !!error?.includes("端点"),
    keyInvalid = !!error?.includes("服务密钥");
  function validate() {
    if (
      !config.display_name.trim() ||
      [...config.display_name.trim()].length > 80
    )
      return "显示名称需为 1–80 个字符。";
    try {
      const u = new URL(config.endpoint);
      if (
        u.username ||
        u.password ||
        u.hash ||
        [...u.searchParams.keys()].some((k) =>
          /key|token|secret|credential|password/i.test(k),
        )
      )
        return "端点不能包含密钥、用户信息或片段。";
      if (config.provider === "volcengine" && config.endpoint !== endpoint)
        return "火山引擎端点需使用已支持的同传地址。";
      if (!["https:", "wss:"].includes(u.protocol))
        return "请输入有效的 HTTPS 或 WSS 端点。";
    } catch {
      return "请输入有效的服务端点。";
    }
    if (
      action === "replace" &&
      (config.provider !== "volcengine" ||
        !key ||
        key.length > 4096 ||
        !/^[!-~]+$/.test(key))
    )
      return "请输入有效的火山引擎服务密钥（最多 4096 字节）。";
    return null;
  }
  async function save() {
    if (disabled) return;
    const invalid = validate();
    if (invalid) {
      setError(invalid);
      return;
    }
    setError(null);
    setNotice(null);
    const credential: CredentialChange =
      action === "replace" ? { action, api_key: key } : { action };
    try {
      const saved = await owner.configuration(
        "save_provider_config",
        {
          request: {
            config: { ...config, display_name: config.display_name.trim() },
            credential,
          },
        },
        decodeProviderConfigView,
      );
      setKey("");
      setAction("keep");
      setConfigured(saved.credential_configured);
      setConfig(saved.config);
      setSelected(saved.config.id);
      setNotice("配置已保存");
      await owner.reload({ fresh: true });
    } catch (e) {
      setError(localDesktopError(e));
    }
  }
  async function remove() {
    try {
      await owner.configuration(
        "delete_provider_config",
        { configId: config.id },
        (value) => {
          if (value !== null) throw Error("invalid delete result");
          return null;
        },
      );
      setConfirm(false);
      load();
      await owner.reload({ fresh: true });
    } catch (e) {
      setError(localDesktopError(e));
    }
  }
  function check() {
    const invalid = validate();
    setError(invalid);
    setNotice(
      invalid
        ? null
        : `本地配置检查通过${configured ? "，密钥已配置" : "，尚未配置密钥"}；尚未验证服务连接。`,
    );
  }
  return (
    <div className="settings-body">
      <section className="settings-section">
        <div>
          <h2>同传服务配置</h2>
          <p>
            保存显示名称、服务端点与密钥。密钥只写入一次，已有密钥不会返回到表单。
          </p>
          <p>
            {config.provider === "aliyun" || config.provider === "palabra"
              ? "未联调"
              : selected
                ? cap?.ready
                  ? "适配器可运行"
                  : "能力未知"
                : "尚未保存"}
          </p>
          {cap?.unavailable_reason && <p>{cap.unavailable_reason}</p>}
          <p>
            Aliyun · 未联调；Palabra · 未联调。目前不能作为可运行的同传服务。
          </p>
        </div>
        <div className="settings-panel">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <FieldGroup>
              <Choice
                id="saved-config"
                label="已保存配置"
                value={selected}
                disabled={disabled || owner.loading}
                options={owner.profiles.map((p) => ({
                  value: p.config.id,
                  label: p.config.display_name,
                }))}
                onChange={(id) =>
                  load(owner.profiles.find((p) => p.config.id === id))
                }
              />
              <Field>
                <Button
                  type="button"
                  variant="outline"
                  disabled={disabled}
                  onClick={() => load()}
                >
                  新增配置
                </Button>
              </Field>
              <Choice
                id="provider-type"
                label="服务商"
                value={config.provider}
                disabled={disabled}
                options={[
                  { value: "volcengine", label: "火山引擎" },
                  { value: "aliyun", label: "Aliyun · 未联调" },
                  { value: "palabra", label: "Palabra · 未联调" },
                  ...(!["volcengine", "aliyun", "palabra"].includes(
                    config.provider,
                  )
                    ? [
                        {
                          value: config.provider,
                          label: `${config.provider} · 未支持`,
                        },
                      ]
                    : []),
                ]}
                onChange={(provider) => {
                  setConfig((old) => ({ ...old, provider }));
                  setKey("");
                  setAction("keep");
                }}
              />
              <Field
                data-disabled={disabled || undefined}
                data-invalid={nameInvalid || undefined}
              >
                <FieldLabel htmlFor="provider-name">显示名称</FieldLabel>
                <Input
                  id="provider-name"
                  aria-invalid={nameInvalid || undefined}
                  value={config.display_name}
                  disabled={disabled}
                  maxLength={160}
                  required
                  onChange={(e) =>
                    setConfig((old) => ({
                      ...old,
                      display_name: e.target.value,
                    }))
                  }
                />
              </Field>
              <Field
                data-disabled={disabled || undefined}
                data-invalid={endpointInvalid || undefined}
              >
                <FieldLabel htmlFor="provider-endpoint">服务端点</FieldLabel>
                <Input
                  id="provider-endpoint"
                  aria-invalid={endpointInvalid || undefined}
                  value={config.endpoint}
                  disabled={disabled}
                  required
                  onChange={(e) =>
                    setConfig((old) => ({ ...old, endpoint: e.target.value }))
                  }
                />
              </Field>
              <Choice
                id="credential-action"
                label="密钥操作"
                value={action}
                disabled={disabled}
                onChange={(v) => {
                  setAction(v as typeof action);
                  setKey("");
                }}
                options={[
                  { value: "keep", label: "保留已有密钥" },
                  {
                    value: "replace",
                    label: "替换密钥",
                    disabled: config.provider !== "volcengine",
                  },
                  { value: "clear", label: "清除密钥" },
                ]}
              />
              <Field
                data-disabled={disabled || action !== "replace" || undefined}
                data-invalid={keyInvalid || undefined}
              >
                <FieldLabel htmlFor="provider-key">服务密钥</FieldLabel>
                <Input
                  id="provider-key"
                  aria-invalid={keyInvalid || undefined}
                  type="password"
                  autoComplete="new-password"
                  value={key}
                  disabled={disabled || action !== "replace"}
                  maxLength={4096}
                  onChange={(e) => setKey(e.target.value)}
                />
                <FieldDescription>
                  {configured ? "密钥已配置" : "密钥未配置"}
                </FieldDescription>
              </Field>
              <Field orientation="horizontal">
                <Button type="submit" disabled={disabled}>
                  {owner.configBusy ? "正在保存" : "保存配置"}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  disabled={disabled}
                  onClick={check}
                >
                  检查配置
                </Button>
              </Field>
            </FieldGroup>
          </form>
          {notice && <p role="status">{notice}</p>}
          {error && (
            <Alert>
              <AlertTitle>配置未完成</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          {owner.locked && <p>停止同传后可更改服务配置。</p>}
          {selected && (
            <div className="settings-destructive">
              <p>删除此配置后，将无法再用它启动同传。</p>
              <Button
                variant="outline"
                disabled={disabled}
                onClick={() => {
                  setError(null);
                  setConfirm(true);
                }}
              >
                删除配置
              </Button>
            </div>
          )}
        </div>
      </section>
      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除“{config.display_name}”？</AlertDialogTitle>
            <AlertDialogDescription>
              将删除此同传配置与关联密钥；已保存的字幕不受影响。
            </AlertDialogDescription>
          </AlertDialogHeader>
          {error && (
            <Alert>
              <AlertTitle>删除未完成</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={owner.configBusy}>
              取消
            </AlertDialogCancel>
            <AlertDialogAction
              variant="outline"
              disabled={disabled}
              onClick={(e) => {
                e.preventDefault();
                void remove();
              }}
            >
              确认删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

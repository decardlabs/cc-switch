import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  providersApi,
  type AppId,
  type Fido2AssertionChallenge,
  type NativeFido2Capability,
  type ProviderSecretStatus,
  type SecretPolicy,
} from "@/lib/api";

interface ProviderSecretPanelProps {
  appId: AppId;
  providerId: string;
  open: boolean;
}

const POLICY_OPTIONS: SecretPolicy[] = [
  "plain",
  "os_keychain",
  "fido2_required",
];

const IS_DEV = import.meta.env.DEV;

export function ProviderSecretPanel({
  appId,
  providerId,
  open,
}: ProviderSecretPanelProps) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<ProviderSecretStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [policy, setPolicy] = useState<SecretPolicy>("plain");
  const [bindApiKey, setBindApiKey] = useState("");
  const [newApiKey, setNewApiKey] = useState("");
  const [fido2Challenge, setFido2Challenge] =
    useState<Fido2AssertionChallenge | null>(null);
  const [fido2Signature, setFido2Signature] = useState("");
  const [nativeFido2Capability, setNativeFido2Capability] =
    useState<NativeFido2Capability | null>(null);
  const [nativeProbeLoading, setNativeProbeLoading] = useState(false);
  const [nowSeconds, setNowSeconds] = useState(() =>
    Math.floor(Date.now() / 1000),
  );

  const loadNativeFido2Capability = useCallback(async () => {
    setNativeProbeLoading(true);
    try {
      const capability = await providersApi.getNativeFido2Capability();
      setNativeFido2Capability(capability);
    } catch (error) {
      console.warn(
        "[ProviderSecretPanel] failed to probe native fido2 capability",
        error,
      );
    } finally {
      setNativeProbeLoading(false);
    }
  }, []);

  const loadStatus = useCallback(async () => {
    if (!open || !providerId) return;
    setLoading(true);
    try {
      const next = await providersApi.getSecretStatus(providerId, appId);
      setStatus(next);
      setPolicy(next.policy);
    } catch (error) {
      console.error("[ProviderSecretPanel] failed to load status", error);
      toast.error(
        t("provider.secret.statusFailed", {
          defaultValue: "读取密钥保护状态失败",
        }),
      );
    } finally {
      setLoading(false);
    }
  }, [open, providerId, appId, t]);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  useEffect(() => {
    if (!fido2Challenge) {
      return;
    }

    const timer = window.setInterval(() => {
      setNowSeconds(Math.floor(Date.now() / 1000));
    }, 1000);

    return () => window.clearInterval(timer);
  }, [fido2Challenge]);

  useEffect(() => {
    if (!open || nativeFido2Capability) {
      return;
    }

    void loadNativeFido2Capability();
  }, [open, nativeFido2Capability, loadNativeFido2Capability]);

  const getFido2UnavailableMessage = useCallback(
    (message?: string) => {
      const msg = message ?? "";
      const normalized = msg.toLowerCase();

      if (msg.includes("FIDO2_NATIVE_PLATFORM_UNSUPPORTED")) {
        return t("provider.secret.fido2UnsupportedPlatform");
      }

      if (msg.includes("FIDO2_NATIVE_NOT_ENABLED")) {
        return t("provider.secret.fido2NativeNotEnabled");
      }

      if (
        msg.includes("未启用原生 FIDO2") ||
        normalized.includes("unavailable") ||
        normalized.includes("disabled") ||
        normalized.includes("fido2")
      ) {
        return t("provider.secret.fido2Unavailable", {
          defaultValue: "FIDO2 后端不可用，请检查当前策略或后端配置",
        });
      }

      return t("provider.secret.fido2BeginFailed", {
        defaultValue: "发起 FIDO2 挑战失败",
      });
    },
    [t],
  );

  const fido2RemainingSeconds = useMemo(() => {
    if (!fido2Challenge) {
      return null;
    }
    return fido2Challenge.expiresAt - nowSeconds;
  }, [fido2Challenge, nowSeconds]);

  const fido2ChallengeExpired =
    fido2RemainingSeconds !== null && fido2RemainingSeconds <= 0;

  const policyLabel = useMemo(() => {
    const current = status?.policy ?? policy;
    if (current === "fido2_required") {
      return t("provider.secret.policyFido2", { defaultValue: "FIDO2 必需" });
    }
    if (current === "os_keychain") {
      return t("provider.secret.policyKeychain", {
        defaultValue: "系统钥匙串",
      });
    }
    return t("provider.secret.policyPlain", { defaultValue: "明文（兼容）" });
  }, [policy, status?.policy, t]);

  useEffect(() => {
    if (!IS_DEV || !nativeFido2Capability) {
      return;
    }

    const activePolicy = status?.policy ?? policy;
    console.debug("[ProviderSecretPanel] native capability observed", {
      appId,
      providerId,
      policy: activePolicy,
      backend: nativeFido2Capability.backend,
      platform: nativeFido2Capability.platform,
      available: nativeFido2Capability.available,
      code: nativeFido2Capability.code ?? "none",
    });
  }, [
    appId,
    providerId,
    policy,
    status?.policy,
    nativeFido2Capability,
  ]);

  const handleEnroll = useCallback(async () => {
    setLoading(true);
    try {
      const result = await providersApi.enrollSecretProtection(
        providerId,
        appId,
        policy,
      );
      toast.success(result.message);
      await loadStatus();
    } catch (error) {
      console.error("[ProviderSecretPanel] enroll failed", error);
      toast.error(
        t("provider.secret.enrollFailed", {
          defaultValue: "初始化密钥保护失败",
        }),
      );
    } finally {
      setLoading(false);
    }
  }, [providerId, appId, policy, loadStatus, t]);

  const handleBind = useCallback(async () => {
    if (!bindApiKey.trim()) {
      toast.error(
        t("provider.secret.bindKeyRequired", {
          defaultValue: "请先输入 API Key",
        }),
      );
      return;
    }
    setLoading(true);
    try {
      const next = await providersApi.bindSecret(
        providerId,
        appId,
        bindApiKey,
        policy,
      );
      setStatus(next);
      setBindApiKey("");
      toast.success(
        t("provider.secret.bindSuccess", { defaultValue: "密钥绑定成功" }),
      );
    } catch (error) {
      console.error("[ProviderSecretPanel] bind failed", error);
      toast.error(
        t("provider.secret.bindFailed", { defaultValue: "密钥绑定失败" }),
      );
    } finally {
      setLoading(false);
    }
  }, [bindApiKey, providerId, appId, policy, t]);

  const handleUnlock = useCallback(async () => {
    const activePolicy = status?.policy ?? policy;
    if (activePolicy === "fido2_required") {
      toast.error(
        t("provider.secret.useFido2Flow", {
          defaultValue: "当前策略需要 FIDO2 挑战，请先发起挑战并提交断言",
        }),
      );
      return;
    }

    setLoading(true);
    try {
      await providersApi.unlockSecret(providerId, appId, "provider_edit");
      toast.success(
        t("provider.secret.unlockSuccess", { defaultValue: "解锁成功" }),
      );
      await loadStatus();
    } catch (error) {
      console.error("[ProviderSecretPanel] unlock failed", error);
      toast.error(
        t("provider.secret.unlockFailed", { defaultValue: "解锁失败" }),
      );
    } finally {
      setLoading(false);
    }
  }, [providerId, appId, loadStatus, t]);

  const handleBeginFido2 = useCallback(async () => {
    setLoading(true);
    try {
      const challenge = await providersApi.beginFido2Assertion(
        providerId,
        appId,
        "provider_edit",
      );
      setFido2Challenge(challenge);
      if (challenge.backend.includes("emulated")) {
        setFido2Signature("emulated-ok");
      }
      toast.success(
        t("provider.secret.fido2BeginSuccess", {
          defaultValue: "FIDO2 挑战已生成，请完成断言后提交验证",
        }),
      );
    } catch (error) {
      console.error("[ProviderSecretPanel] begin fido2 failed", error);
      setFido2Challenge(null);
      setFido2Signature("");

      const message =
        error instanceof Error ? error.message : String(error ?? "");

      toast.error(getFido2UnavailableMessage(message));
    } finally {
      setLoading(false);
    }
  }, [providerId, appId, getFido2UnavailableMessage]);

  const handleVerifyFido2 = useCallback(async () => {
    if (!fido2Challenge) {
      toast.error(
        t("provider.secret.fido2NoChallenge", {
          defaultValue: "请先发起 FIDO2 挑战",
        }),
      );
      return;
    }
    if (!fido2Signature.trim()) {
      toast.error(
        t("provider.secret.fido2SignatureRequired", {
          defaultValue: "请输入断言签名",
        }),
      );
      return;
    }
    if (fido2ChallengeExpired) {
      setFido2Challenge(null);
      setFido2Signature("");
      toast.error(
        t("provider.secret.fido2ChallengeExpired", {
          defaultValue: "挑战已过期，请重新发起 FIDO2 挑战",
        }),
      );
      return;
    }

    setLoading(true);
    try {
      await providersApi.verifyFido2Assertion(
        providerId,
        appId,
        fido2Challenge.challengeId,
        fido2Signature,
      );
      setFido2Challenge(null);
      setFido2Signature("");
      toast.success(
        t("provider.secret.fido2VerifySuccess", {
          defaultValue: "FIDO2 验证成功，密钥已解锁",
        }),
      );
      await loadStatus();
    } catch (error) {
      console.error("[ProviderSecretPanel] verify fido2 failed", error);

      setFido2Challenge(null);
      setFido2Signature("");

      const message =
        error instanceof Error ? error.message : String(error ?? "");
      const normalized = message.toLowerCase();
      const isExpired =
        message.includes("过期") || normalized.includes("expired");
      const isInvalidSignature =
        message.includes("签名") ||
        normalized.includes("signature") ||
        normalized.includes("assertion") ||
        normalized.includes("invalid");
      const isBackendUnavailable =
        message.includes("FIDO2_NATIVE_PLATFORM_UNSUPPORTED") ||
        message.includes("FIDO2_NATIVE_NOT_ENABLED") ||
        message.includes("未启用原生 FIDO2") ||
        normalized.includes("unavailable") ||
        normalized.includes("disabled");

      toast.error(
        isExpired
          ? t("provider.secret.fido2ChallengeExpired", {
              defaultValue: "挑战已过期，请重新发起 FIDO2 挑战",
            })
          : isInvalidSignature
            ? t("provider.secret.fido2InvalidSignature", {
                defaultValue: "断言签名无效，请重新发起挑战后重试",
              })
            : isBackendUnavailable
              ? getFido2UnavailableMessage(message)
              : t("provider.secret.fido2VerifyFailed", {
                  defaultValue: "FIDO2 验证失败",
                }),
      );
    } finally {
      setLoading(false);
    }
  }, [
    fido2Challenge,
    fido2Signature,
    fido2ChallengeExpired,
    providerId,
    appId,
    getFido2UnavailableMessage,
    loadStatus,
    t,
  ]);

  const handleRotate = useCallback(async () => {
    if (!newApiKey.trim()) {
      toast.error(
        t("provider.secret.rotateKeyRequired", {
          defaultValue: "请输入新的 API Key",
        }),
      );
      return;
    }
    setLoading(true);
    try {
      const next = await providersApi.rotateSecret(providerId, appId, newApiKey);
      setStatus(next);
      setNewApiKey("");
      toast.success(
        t("provider.secret.rotateSuccess", { defaultValue: "密钥轮换成功" }),
      );
    } catch (error) {
      console.error("[ProviderSecretPanel] rotate failed", error);
      toast.error(
        t("provider.secret.rotateFailed", { defaultValue: "密钥轮换失败" }),
      );
    } finally {
      setLoading(false);
    }
  }, [newApiKey, providerId, appId, t]);

  return (
    <Card className="border-border/60 bg-card/50 mt-6">
      <CardHeader className="pb-3">
        <CardTitle className="text-base flex items-center gap-2">
          {t("provider.secret.title", { defaultValue: "密钥保护（Phase 0）" })}
          <Badge variant={status?.hasSecret ? "default" : "outline"}>
            {status?.hasSecret
              ? t("provider.secret.bound", { defaultValue: "已绑定" })
              : t("provider.secret.unbound", { defaultValue: "未绑定" })}
          </Badge>
        </CardTitle>
      </CardHeader>

      <CardContent className="space-y-3">
        <div className="text-sm text-muted-foreground flex items-center gap-2">
          <span>
            {t("provider.secret.currentPolicy", { defaultValue: "当前策略" })}
          </span>
          <Badge variant="outline">{policyLabel}</Badge>
          {status?.canUseFido2 ? (
            <Badge variant="secondary">
              {t("provider.secret.fido2Available", {
                defaultValue: "FIDO2 可用",
              })}
            </Badge>
          ) : (
            <Badge variant="outline">
              {t("provider.secret.fido2Unavailable", {
                defaultValue: "FIDO2 未启用",
              })}
            </Badge>
          )}
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <Select
            value={policy}
            onValueChange={(value) => setPolicy(value as SecretPolicy)}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {POLICY_OPTIONS.map((option) => (
                <SelectItem value={option} key={option}>
                  {option}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Button variant="outline" onClick={handleEnroll} disabled={loading}>
            {t("provider.secret.enroll", { defaultValue: "初始化保护" })}
          </Button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <Input
            type="password"
            placeholder={t("provider.secret.bindPlaceholder", {
              defaultValue: "输入 API Key 并绑定",
            })}
            value={bindApiKey}
            onChange={(e) => setBindApiKey(e.target.value)}
          />
          <Button onClick={handleBind} disabled={loading}>
            {t("provider.secret.bind", { defaultValue: "绑定密钥" })}
          </Button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <Input
            type="password"
            placeholder={t("provider.secret.rotatePlaceholder", {
              defaultValue: "输入新 API Key 进行轮换",
            })}
            value={newApiKey}
            onChange={(e) => setNewApiKey(e.target.value)}
          />
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={handleUnlock}
              disabled={loading}
              className="flex-1"
            >
              {t("provider.secret.unlock", { defaultValue: "解锁" })}
            </Button>
            <Button onClick={handleRotate} disabled={loading} className="flex-1">
              {t("provider.secret.rotate", { defaultValue: "轮换" })}
            </Button>
          </div>
        </div>

        {(status?.policy ?? policy) === "fido2_required" ? (
          <div className="space-y-3 border border-border/50 rounded-md p-3">
            <div className="text-xs text-muted-foreground">
              {t("provider.secret.fido2Hint", {
                defaultValue:
                  "FIDO2 策略下请先发起挑战，再提交断言签名完成解锁。仿真模式签名可使用 emulated-ok。",
              })}
            </div>

            {nativeFido2Capability && !nativeFido2Capability.available ? (
              <div className="text-xs text-muted-foreground">
                {getFido2UnavailableMessage(nativeFido2Capability.code)}
              </div>
            ) : null}

            {IS_DEV && nativeFido2Capability?.reason ? (
              <div className="text-xs text-muted-foreground/80 break-all">
                {nativeFido2Capability.reason}
              </div>
            ) : null}

            {IS_DEV ? (
              <Button
                variant="outline"
                onClick={loadNativeFido2Capability}
                disabled={loading || nativeProbeLoading}
              >
                {t("provider.secret.fido2RefreshProbe", {
                  defaultValue: "刷新 FIDO2 探测",
                })}
              </Button>
            ) : null}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <Button
                variant="outline"
                onClick={handleBeginFido2}
                disabled={loading}
              >
                {t("provider.secret.fido2Begin", {
                  defaultValue: "发起 FIDO2 挑战",
                })}
              </Button>
              <Input
                placeholder={t("provider.secret.fido2ChallengeId", {
                  defaultValue: "挑战 ID",
                })}
                value={fido2Challenge?.challengeId ?? ""}
                readOnly
              />
            </div>

            {fido2Challenge ? (
              <div className="text-xs text-muted-foreground">
                {fido2ChallengeExpired
                  ? t("provider.secret.fido2ChallengeExpired", {
                      defaultValue: "挑战已过期，请重新发起 FIDO2 挑战",
                    })
                  : t("provider.secret.fido2ExpiresIn", {
                      defaultValue: "挑战将在 {{seconds}} 秒后过期",
                      seconds: Math.max(0, fido2RemainingSeconds ?? 0),
                    })}
              </div>
            ) : null}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <Input
                placeholder={t("provider.secret.fido2Signature", {
                  defaultValue: "输入断言签名",
                })}
                value={fido2Signature}
                onChange={(e) => setFido2Signature(e.target.value)}
              />
              <Button
                onClick={handleVerifyFido2}
                disabled={loading || !fido2Challenge || fido2ChallengeExpired}
              >
                {t("provider.secret.fido2Verify", {
                  defaultValue: "验证断言并解锁",
                })}
              </Button>
            </div>
          </div>
        ) : null}

        {status?.message ? (
          <div className="text-xs text-muted-foreground">{status.message}</div>
        ) : null}
      </CardContent>
    </Card>
  );
}

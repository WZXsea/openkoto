import { FormEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Lock, LogIn, RefreshCw, Server, UserPlus } from "lucide-react";

import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Label } from "../ui/label";
import type { AppConfig, BackendAuthResult, BackendSessionCheck } from "../../lib/tauri";

type AuthMode = "login" | "register";

interface BackendConnectionGateProps {
  config: AppConfig | null;
  status: BackendSessionCheck | null;
  isChecking: boolean;
  onRetry: () => Promise<unknown>;
  onAuthenticated: (config: AppConfig) => Promise<void> | void;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "请求失败";
}

export function BackendConnectionGate({
  config,
  status,
  isChecking,
  onRetry,
  onAuthenticated,
}: BackendConnectionGateProps) {
  const [mode, setMode] = useState<AuthMode>("login");
  const detectedBackendUrl = status?.backend_url || config?.backend_url || null;
  const [backendUrl, setBackendUrl] = useState(detectedBackendUrl || "http://127.0.0.1:4000");
  const [hasEditedBackendUrl, setHasEditedBackendUrl] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  useEffect(() => {
    if (!hasEditedBackendUrl && detectedBackendUrl && detectedBackendUrl !== backendUrl) {
      setBackendUrl(detectedBackendUrl);
    }
  }, [backendUrl, detectedBackendUrl, hasEditedBackendUrl]);

  const statusText = useMemo(() => {
    if (isChecking) return "正在检查 Backend";
    if (!status?.configured) return "需要配置 Backend 地址";
    if (!status.connected) return "Backend 无法连接";
    if (status.error?.includes("authentication is not configured")) return "Backend 认证未启用";
    if (!status.authenticated) return "需要登录 Backend";
    return "Backend 已连接";
  }, [isChecking, status]);

  const canSubmit = backendUrl.trim() && email.trim() && password.length >= 1 && !isSubmitting;
  const submitLabel = mode === "login" ? "登录" : "注册并登录";
  const toggleLabel = mode === "login" ? "创建账户" : "使用已有账户";

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    if (!canSubmit) return;

    setIsSubmitting(true);
    setLocalError(null);
    try {
      const command = mode === "login" ? "backend_login_cmd" : "backend_register_cmd";
      const result = await invoke<BackendAuthResult>(command, {
        backendUrl,
        email,
        password,
        displayName: mode === "register" ? displayName || null : null,
      });
      await onAuthenticated(result.config);
    } catch (error) {
      setLocalError(errorMessage(error));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="h-screen bg-background text-foreground flex items-center justify-center px-6">
      <div className="w-full max-w-md border border-border rounded-lg bg-card p-6 shadow-sm">
        <div className="flex items-start gap-3 mb-5">
          <div className="h-10 w-10 rounded-lg bg-primary text-primary-foreground flex items-center justify-center">
            {status?.connected ? <Lock size={20} /> : <Server size={20} />}
          </div>
          <div className="min-w-0">
            <h1 className="text-lg font-semibold leading-6">OpenKoto Backend</h1>
            <p className="text-sm text-muted-foreground mt-1">{statusText}</p>
          </div>
        </div>

        {(status?.error || localError) && (
          <div className="mb-4 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {localError || status?.error}
          </div>
        )}

        <form className="space-y-4" onSubmit={handleSubmit}>
          <div className="space-y-2">
            <Label htmlFor="backend-url">Backend URL</Label>
            <Input
              id="backend-url"
              value={backendUrl}
              onChange={(event) => {
                setHasEditedBackendUrl(true);
                setBackendUrl(event.target.value);
              }}
              placeholder="http://127.0.0.1:4000"
              autoCapitalize="none"
              autoCorrect="off"
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="backend-email">Email</Label>
            <Input
              id="backend-email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              type="email"
              autoCapitalize="none"
              autoCorrect="off"
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="backend-password">Password</Label>
            <Input
              id="backend-password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              type="password"
            />
          </div>

          {mode === "register" && (
            <div className="space-y-2">
              <Label htmlFor="backend-display-name">Display name</Label>
              <Input
                id="backend-display-name"
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
              />
            </div>
          )}

          <div className="flex items-center gap-2 pt-1">
            <Button type="submit" disabled={!canSubmit} className="gap-2">
              {mode === "login" ? <LogIn size={16} /> : <UserPlus size={16} />}
              {isSubmitting ? "处理中" : submitLabel}
            </Button>
            <Button type="button" variant="secondary" onClick={() => setMode(mode === "login" ? "register" : "login")}>
              {toggleLabel}
            </Button>
            <Button type="button" variant="ghost" onClick={() => { void onRetry(); }} disabled={isChecking} title="重新检查">
              <RefreshCw size={16} />
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}

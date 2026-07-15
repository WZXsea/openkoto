import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { AppConfig, BackendSessionCheck } from "../../lib/tauri";
import { BackendConnectionGate } from "./BackendConnectionGate";

afterEach(cleanup);

const CONFIG: AppConfig = {
  model_configs: [],
  target_language: "zh-CN",
  interface_language: "zh",
  backend_url: "http://127.0.0.1:19421",
};

const OFFLINE_STATUS: BackendSessionCheck = {
  configured: true,
  connected: false,
  authenticated: false,
  backend_url: CONFIG.backend_url,
  error: "connection refused",
};

describe("BackendConnectionGate", () => {
  it("exposes offline diagnostics without hiding the recoverable login form", async () => {
    render(
      <BackendConnectionGate
        config={CONFIG}
        status={OFFLINE_STATUS}
        isChecking={false}
        onRetry={vi.fn().mockResolvedValue(undefined)}
        onAuthenticated={vi.fn()}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("connection refused");
    expect(screen.getByLabelText("Backend URL")).toHaveValue("http://127.0.0.1:19421");
    expect(screen.getByLabelText("Email")).toBeEnabled();
    expect(screen.getByLabelText("Password")).toBeEnabled();

    await userEvent.click(screen.getByText("连接诊断"));
    expect(screen.getByText("未连接")).toBeInTheDocument();
    expect(screen.getByText("未登录")).toBeInTheDocument();
  });

  it("reports retry failures locally and allows another connection check", async () => {
    const onRetry = vi.fn()
      .mockRejectedValueOnce(new Error("Backend still offline"))
      .mockResolvedValueOnce(undefined);

    render(
      <BackendConnectionGate
        config={CONFIG}
        status={OFFLINE_STATUS}
        isChecking={false}
        onRetry={onRetry}
        onAuthenticated={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "重新检查" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Backend still offline");
    expect(screen.getByRole("button", { name: "重新检查" })).toBeEnabled();

    await userEvent.click(screen.getByRole("button", { name: "重新检查" }));
    await waitFor(() => expect(onRetry).toHaveBeenCalledTimes(2));
  });
});

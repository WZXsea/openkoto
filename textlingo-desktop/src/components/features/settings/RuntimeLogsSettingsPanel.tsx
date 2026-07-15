import { LogsPanel } from "../LogsPanel";

export function RuntimeLogsSettingsPanel() {
  return (
    <section className="h-full" aria-label="运行日志">
      <LogsPanel />
    </section>
  );
}

import type { FeatureBoundary } from "../types";

export const settingsFeature = {
  id: "settings",
  label: "Settings",
  entry: "src/features/settings",
  status: "shell-only",
  phase1Scope: "Settings dialog, onboarding, provider quick switching, and update-check UI boundaries.",
  legacyComponents: [
    "src/components/features/SettingsDialog.tsx",
    "src/components/features/OnboardingDialog.tsx",
    "src/components/features/ApiQuickSwitcher.tsx",
    "src/components/features/UpdateChecker.tsx",
  ],
  notes: [
    "Settings internals remain in legacy components during PR-2.",
    "New external connectors require a later phase decision.",
  ],
} as const satisfies FeatureBoundary;

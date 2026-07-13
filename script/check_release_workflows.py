#!/usr/bin/env python3

from pathlib import Path
import re
import sys


REQUIRED_CI_GATE_SNIPPETS = (
    'uses: actions/checkout@v5',
    'uses: actions/setup-node@v5',
    'uses: actions/setup-python@v6',
    'name: setup python',
    'name: build bundled PDF sidecar',
    'name: verify bundled PDF sidecar',
    'bash script/verify_pdf_sidecar_binary.sh',
    'name: Install agent worker dependencies',
    'working-directory: ./textlingo-desktop/agent-worker',
)

REQUIRED_PUBLISH_SNIPPETS = (
    'uses: actions/checkout@v5',
    'uses: actions/setup-node@v5',
    'uses: actions/setup-python@v6',
    'name: setup python',
    'name: build bundled PDF sidecar',
    'name: verify bundled PDF sidecar',
    'bash script/verify_pdf_sidecar_binary.sh',
    'name: install agent worker dependencies',
    'working-directory: ./textlingo-desktop/agent-worker',
)

REQUIRED_MACOS_MATRIX_ROWS = (
    '- platform: "macos-14"\n            args: "--target aarch64-apple-darwin --bundles app,dmg"',
    '- platform: "macos-15-intel"\n            args: "--target x86_64-apple-darwin --bundles app,dmg"',
)

LEGACY_MACOS_PLATFORM_SNIPPETS = (
    'matrix.platform == \'macos-latest\'',
    'matrix.platform == "macos-latest"',
)


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    workflow_paths = (
        root / ".github/workflows/release.yml",
        root / ".github/workflows/release-dev.yml",
    )

    missing = []
    for workflow_path in workflow_paths:
        content = workflow_path.read_text()
        ci_gate_match = re.search(r"(?ms)^  ci-gate:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)", content)
        if ci_gate_match is None:
            missing.append(f"{workflow_path}: missing `ci-gate` job")
            continue
        ci_gate_body = ci_gate_match.group("body")
        for snippet in REQUIRED_CI_GATE_SNIPPETS:
            if snippet not in ci_gate_body:
                missing.append(f"{workflow_path}: ci-gate missing `{snippet}`")

        publish_tauri_match = re.search(
            r"(?ms)^  publish-tauri:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
            content,
        )
        if publish_tauri_match is None:
            missing.append(f"{workflow_path}: missing `publish-tauri` job")
            continue

        publish_tauri_body = publish_tauri_match.group("body")
        for snippet in REQUIRED_PUBLISH_SNIPPETS:
            if snippet not in publish_tauri_body:
                missing.append(f"{workflow_path}: publish-tauri missing `{snippet}`")
        for row in REQUIRED_MACOS_MATRIX_ROWS:
            if row not in publish_tauri_body:
                missing.append(f"{workflow_path}: publish-tauri missing macOS matrix row `{row}`")

        for legacy_snippet in LEGACY_MACOS_PLATFORM_SNIPPETS:
            if legacy_snippet in publish_tauri_body:
                missing.append(
                    f"{workflow_path}: publish-tauri still references legacy macOS runner snippet `{legacy_snippet}`"
                )

    if missing:
        print("\n".join(missing))
        return 1

    print("release workflows install all Node workspaces, verify sidecars, and publish the macOS app and DMG")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

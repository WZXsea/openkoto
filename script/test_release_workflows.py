#!/usr/bin/env python3

import os
import json
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RELEASE_WORKFLOWS = (
    ROOT / ".github" / "workflows" / "release.yml",
    ROOT / ".github" / "workflows" / "release-dev.yml",
)

EXPECTED_MACOS_ARGS = (
    'args: "--target aarch64-apple-darwin --bundles app,dmg"',
    'args: "--target x86_64-apple-darwin --bundles app,dmg"',
)

LEGACY_MACOS_ARGS = (
    'args: "--target aarch64-apple-darwin --bundles app"',
    'args: "--target x86_64-apple-darwin --bundles app"',
)


class ReleaseWorkflowTests(unittest.TestCase):
    def test_release_version_gate_accepts_the_wzx_prerelease_tag(self) -> None:
        env = os.environ.copy()
        package = json.loads((ROOT / "textlingo-desktop" / "package.json").read_text())
        env["OPENKOTO_RELEASE_TAG"] = f"wzx-v{package['version']}-workflow-test"
        result = subprocess.run(
            ["bash", "script/verify_release_version.sh"],
            cwd=ROOT,
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_release_workflows_publish_app_and_dmg_for_macos(self) -> None:
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()

            for expected_arg in EXPECTED_MACOS_ARGS:
                self.assertIn(expected_arg, content, f"{workflow} missing `{expected_arg}`")

            for legacy_arg in LEGACY_MACOS_ARGS:
                self.assertNotIn(legacy_arg, content, f"{workflow} still contains `{legacy_arg}`")

    def test_release_workflows_execute_pdf_sidecar_verification(self) -> None:
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()
            self.assertIn(
                "bash script/verify_pdf_sidecar_binary.sh",
                content,
                f"{workflow} must execute the bundled PDF sidecar, not just list it",
            )

    def test_release_workflows_install_agent_worker_dependencies(self) -> None:
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()
            self.assertGreaterEqual(
                content.count("working-directory: ./textlingo-desktop/agent-worker"),
                2,
                f"{workflow} must install agent-worker dependencies in ci-gate and publish-tauri",
            )
            self.assertNotIn(
                "run: npm install",
                content,
                f"{workflow} must use reproducible npm ci installs",
            )

    def test_release_workflows_build_worker_and_test_backend_in_ci_gate(self) -> None:
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()
            required = (
                "name: Build and test agent worker",
                "npm run typecheck && npm run test && npm run build",
                "name: Backend service checks and tests",
                "cargo test --manifest-path openkoto-backend/Cargo.toml --no-fail-fast",
                "OPENKOTO_TEST_DATABASE_URL:",
                "name: Build frontend production bundle",
                "name: Desktop Rust format, check, and tests",
                "cargo fmt --all -- --check",
                "cargo check --all-features --all-targets",
                "cargo test --all-features --all-targets",
                "name: Install Playwright browser",
                "npx playwright install --with-deps chromium",
                "name: Playwright E2E",
                "npm run e2e",
            )
            for entry in required:
                self.assertIn(entry, content, f"{workflow} missing release gate `{entry}`")

    def test_release_workflows_fail_fast_when_apple_credentials_are_missing(self) -> None:
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()
            required = (
                "name: verify Apple release credentials (macos only)",
                "APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY",
                "APPLE_API_KEY_CONTENT APPLE_API_ISSUER APPLE_API_KEY APPLE_TEAM_ID",
                'echo "::error::Missing Apple release secrets: ${missing[*]}"',
                "base64 -D > certificate.p12",
            )
            for entry in required:
                self.assertIn(entry, content, f"{workflow} missing credential preflight `{entry}`")

    def test_release_workflows_validate_their_tag_version(self) -> None:
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()
            self.assertIn("OPENKOTO_RELEASE_TAG: ${{ github.ref_name }}", content)
            self.assertIn("bash script/verify_release_version.sh", content)

    def test_release_workflows_verify_all_packaged_runtime_resources(self) -> None:
        required = (
            "Contents/Resources/backend/openkoto-backend",
            "Contents/Resources/postgres/bin/postgres",
            "Contents/Resources/postgres/bin/initdb",
            "Contents/Resources/node/bin/node",
            "Contents/Resources/agent-worker/dist/index.js",
            'codesign --verify --deep --strict --verbose=2 "$APP"',
            'spctl --assess --type execute --verbose=4 "$APP"',
            'xcrun stapler validate "$APP"',
            'hdiutil verify "$DMG"',
            'spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG"',
            'xcrun stapler validate "$DMG"',
        )
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()
            for entry in required:
                self.assertIn(entry, content, f"{workflow} missing packaged resource check `{entry}`")

    def test_release_workflows_publish_only_after_matrix_validation(self) -> None:
        for workflow in RELEASE_WORKFLOWS:
            content = workflow.read_text()
            self.assertIn("releaseDraft: true", content)
            self.assertIn("finalize-release:", content)
            self.assertIn("needs: publish-tauri", content)
            self.assertLess(content.index("releaseDraft: true"), content.index("finalize-release:"))

        self.assertIn("--prerelease=false --latest", RELEASE_WORKFLOWS[0].read_text())
        self.assertIn("--prerelease=true", RELEASE_WORKFLOWS[1].read_text())


if __name__ == "__main__":
    unittest.main()

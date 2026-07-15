# OpenKoto Desktop Release Guide

This guide describes the current OpenKoto Desktop packaging and release process.

## 0.10.0 PR-12 Packaging

Version `0.10.0` adds the observable Assistant workflow: global and reader-scoped task
views, durable timelines, cancellation, retry lineage, artifact viewing, source return,
and audited in-app actions. External software writes remain disabled in phase one.

The WZX Apple Silicon prerelease is published from
`wzx/pr12-assistant-observability` with tag
`wzx-v0.10.0-pr12-assistant-observability`. Before upload, run the PR-12 isolated
database gate, historical regression chain, packaged runtime smoke, checksum
verification, and local 0.9.0-to-0.10.0 upgrade validation.

## 0.9.0 PR-11 Packaging

Version `0.9.0` adds the reading-first home, responsive left navigation, dedicated
material library, source-aware reader return, and twelve-week activity heatmap.

The WZX Apple Silicon prerelease is published from the verified
`wzx/pr11-reading-home-shell` branch with the descriptive tag
`wzx-v0.9.0-pr11-reading-home-shell`. The package must pass the PR-11 gate, packaged
runtime smoke, checksum verification, and local upgrade validation before upload.

## 0.8.0 PR-10 Packaging

Version `0.8.0` adds the canonical learning-item state machine, the learning workbench,
legacy favorite migration preview/commit, activity events, and local review summaries.

The WZX Apple Silicon prerelease is published from the verified
`wzx/pr10-learning-domain-local-review` branch with the descriptive tag
`wzx-v0.8.0-pr10-learning-workbench`. Descriptive `wzx-*` tags do not trigger the
repository release workflows, so this package is built and verified locally before the
DMG, checksum, and verification notes are uploaded with `gh release create --prerelease`.

## 0.7.0 Packaging

Version `0.7.0` packages the Axum backend, PostgreSQL 16 runtime, Node.js runtime, and agent worker into the macOS application bundle.

The WZX PR-7 host-built Apple Silicon package is published as the prerelease tag `wzx-v0.7.0-pr7-material-workbench`. It is intentionally separate from the signed upstream `v0.7.0` workflow. User installation and data handling are documented in [docs/INSTALL_MACOS.md](docs/INSTALL_MACOS.md).

## How to Build and Release

GitHub Actions currently builds macOS packages for Apple Silicon and Intel. Windows and Linux are excluded until their packaged PostgreSQL lifecycle is implemented and verified.

### 1. Tag a Release
The build workflow is triggered when you push a tag starting with `v`.

```bash
git tag v0.10.0
git push origin v0.10.0
```

### 2. Monitor Build
Go to the "Actions" tab in your GitHub repository to see the build progress.
There will be a "publish" workflow running.

### 3. Download Assets
Once the build is complete, a published GitHub Release is created in the "Releases" section.
It will contain:
- **macOS Apple Silicon**: `.dmg`, `.app.tar.gz`
- **macOS Intel**: `.dmg`, `.app.tar.gz`

## Local Build (macOS)
To build the macOS version locally for testing:

```bash
cd textlingo-desktop
npm ci
npm --prefix agent-worker ci
npm run tauri:build:packaged
```
The output will be in `src-tauri/target/release/bundle/dmg`.

Run the PR-12 acceptance gate and historical regression chain from the repository root:

```bash
bash script/verify_pr12_assistant_observability.sh --full --start-db
bash script/verify_pr12_assistant_observability.sh --run-pr11
```

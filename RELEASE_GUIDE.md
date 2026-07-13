# OpenKoto Desktop Release Guide

This guide describes the current OpenKoto Desktop packaging and release process.

## 0.7.0 Packaging

Version `0.7.0` packages the Axum backend, PostgreSQL 16 runtime, Node.js runtime, and agent worker into the macOS application bundle.

The WZX PR-7 host-built Apple Silicon package is published as the prerelease tag `wzx-v0.7.0-pr7-material-workbench`. It is intentionally separate from the signed upstream `v0.7.0` workflow. User installation and data handling are documented in [docs/INSTALL_MACOS.md](docs/INSTALL_MACOS.md).

## How to Build and Release

GitHub Actions currently builds macOS packages for Apple Silicon and Intel. Windows and Linux are excluded until their packaged PostgreSQL lifecycle is implemented and verified.

### 1. Tag a Release
The build workflow is triggered when you push a tag starting with `v`.

```bash
git tag v0.7.0
git push origin v0.7.0
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

Run the full PR-7 acceptance gate, including app and DMG resource validation, from the repository root:

```bash
bash script/verify_pr7_material_workbench.sh --build-app
```

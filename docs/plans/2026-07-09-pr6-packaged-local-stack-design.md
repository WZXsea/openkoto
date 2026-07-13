# PR-6 Packaged Local Stack Design
## Status
本设计已在提交 `13b5b58` 中形成代码基线。2026-07-13 的 `0.7.0` 工作区已完成固定 Backend 地址 `http://127.0.0.1:19421`、PID/残留进程治理、账户可见性、发行资源构建和版本一致性门禁。完整自动化与安装版 smoke 均通过：全新素材经预览导入成功，检索唯一命中，阅读进度写入 33%，Desktop、Backend、PostgreSQL 全栈停止后重新启动仍保持账户、素材和阅读位置。
## Goal
把 PR-5 之后的 OpenKoto 改造成接近原作者发行体验的桌面应用：用户下载 macOS `.dmg` 后拖入 Applications，打开 `OpenKoto.app` 即可使用，不需要手动运行 `cargo run openkoto-backend`、Docker PostgreSQL 或填写后端地址。
## Target Runtime
```mermaid
flowchart TD
  A["OpenKoto.app"] --> B["Desktop service manager"]
  B --> C["Bundled openkoto-backend resource binary"]
  B --> D["Bundled PostgreSQL runtime"]
  B --> J["Bundled Node runtime for agent-worker"]
  C --> E["PostgreSQL data dir"]
  C --> F["Backend file storage"]
  A --> G["Reader / candidate inbox UI"]
  E --> H["~/Library/Application Support/OpenKoto Desktop/backend/postgres-data"]
  F --> I["~/Library/Application Support/OpenKoto Desktop/backend/files"]
```
## PR-6 Scope
PR-6 不改变 PR-4/PR-5 的 Backend-first 数据模型。桌面端仍通过 Axum Backend 访问 PostgreSQL；本 PR 只把这些服务纳入 `.app` 的本地生命周期管理。
## Subphase Plan
| Phase | Goal | User-visible result | Current status | Verification |
|---|---|---|---|---|
| PR-6.1 | Backend binary packaging and startup manager | `.app` 自动启动 bundled `openkoto-backend` | 已完成代码基线 | backend build script, Rust tests, desktop `cargo check/test`, packaged config check |
| PR-6.2 | Bundled PostgreSQL runtime | 首次启动自动 `initdb`，之后自动启动本地 PostgreSQL | 已完成代码基线 | PostgreSQL smoke test, migration smoke test, restart test |
| PR-6.3 | First-run local account and worker runtime | 用户不填写 backend URL；agent-worker 不依赖系统 Node | 已完成代码基线，账户 UI 在 PR-6.5 完善 | Tauri smoke flow, bundled Node check |
| PR-6.4 | `.dmg` release smoke test | `.dmg` 安装后能创建文章、记录阅读进度、重启后数据仍在 | 本机已验证；后续固定为发行门禁 | install/open/smoke/restart script |
## PR-6.1 Design
### Backend resource binary
`openkoto-backend` 同步为两类发行文件：
```text
textlingo-desktop/src-tauri/binaries/openkoto-backend-<target-triple>
textlingo-desktop/src-tauri/resources/backend/openkoto-backend
```
`binaries/` 文件用于保留 Tauri sidecar 兼容命名；`resources/backend/` 文件是 macOS `.app` 运行时实际读取的路径：
```text
OpenKoto.app/Contents/Resources/backend/openkoto-backend
```
Tauri resources 使用静态配置：
```json
{
  "bundle": {
    "resources": {
      "resources/backend": "backend"
    }
  }
}
```
默认 `tauri.conf.json` 继续保留空 `externalBin`，保持 core build 和 Phase 1 安全测试不受影响。前端 shell 权限不放宽，backend 只由 Rust service manager 启动。
### Desktop service manager
Tauri Rust 层新增 packaged backend manager：
1. 发行版默认启用，开发版默认关闭。
2. 开发版可通过 `OPENKOTO_DESKTOP_AUTOSTART_BACKEND=1` 手动开启。
3. 自动创建：
   - `backend/`
   - `backend/files/`
   - `backend/jwt_secret`
4. 发行版固定监听 `127.0.0.1:19421`，用户不需要查看或填写端口。
5. 启动 backend resource binary 时注入：
   - `OPENKOTO_BACKEND_BIND`
   - `OPENKOTO_JWT_SECRET`
   - `OPENKOTO_FILE_STORAGE_DIR`
   - `DATABASE_URL`
6. 自动把 `backend_url` 写入 desktop `config.json`。
### Database boundary in PR-6.1
PR-6.1 最初不内置 PostgreSQL。当前完成 PR-6.2 后，数据库连接优先级为：
1. `OPENKOTO_PACKAGED_DATABASE_URL`
2. bundled PostgreSQL runtime
3. Debug 构建允许 `DATABASE_URL`
4. Debug 构建 fallback：`postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev`
发行构建不读取普通 `DATABASE_URL`，避免用户 shell 环境意外改变应用数据库。
## PR-6.2 PostgreSQL Bundle Design
目标资源布局：
```text
OpenKoto.app/Contents/Resources/postgres/
  bin/postgres
  bin/initdb
  bin/createdb
  bin/pg_ctl
  lib/
  share/postgresql@16/
```
本地数据布局：
```text
~/Library/Application Support/OpenKoto Desktop/backend/postgres-data
~/Library/Application Support/OpenKoto Desktop/backend/postgres-socket
```
启动流程：
1. 检查 `backend/postgres-data/PG_VERSION`。
2. 不存在则执行 `initdb`。
3. 动态选择本地端口。
4. 启动 PostgreSQL。
5. 通过 bundled `createdb` 幂等创建 `openkoto` 数据库。
6. 将 `DATABASE_URL` 传给 backend sidecar。
7. backend 启动时自动执行 SQLx migration。
数据库连接优先级：
1. `OPENKOTO_PACKAGED_DATABASE_URL`
2. `.app/Contents/Resources/postgres` 中的 bundled PostgreSQL runtime
3. Debug 构建的 `DATABASE_URL`
4. Debug 构建 fallback：`postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev`
## PR-6.3 Node Runtime Design
agent-worker 仍使用现有 `agent-worker/dist` 和 `agent-worker/node_modules` 资源，但执行器优先读取：
```text
OpenKoto.app/Contents/Resources/node/bin/node
```
回退顺序：
1. bundled Node runtime
2. `TEXTLINGO_AGENT_WORKER_NODE`
3. 常见系统路径 `/opt/homebrew/bin/node`、`/usr/local/bin/node`、`/usr/bin/node`
4. `node`
这样普通用户不需要先安装 Node 才能使用内置 agent-worker。
## Verification Plan
### PR-6.1
```bash
bash script/verify_pr6_packaged_local_stack.sh
```
覆盖：
1. PR-6 文档存在。
2. backend binary 可构建到 Tauri binaries 和 resources/backend 目录。
3. packaged build script 会先构建 backend resource。
4. Desktop Rust `cargo check/test`。
5. Frontend `npm run typecheck`。
### PR-6.2+
```bash
bash script/verify_pr6_packaged_local_stack.sh --build-dmg
```
覆盖：
1. `.dmg` 产物存在。
2. `.app` 内含 backend 和 PostgreSQL runtime。
3. 安装后首次启动能初始化数据库。
4. backend health ready。
5. 创建素材、划词候选、接受到词包。
6. 退出重启后数据仍在。
## Release Risks
| Risk | Mitigation |
|---|---|
| `.dmg` 体积变大 | 接受 PostgreSQL runtime 作为已确定架构成本，通过 release asset 和增量升级策略控制分发成本 |
| macOS signing fails on nested binaries | 所有 sidecar 和 PostgreSQL binaries 进入 signing inventory |
| Backend fixed port conflict | 固定使用 `127.0.0.1:19421`；只清理由本应用记录或匹配应用数据目录的残留进程，其他占用进入诊断错误 |
| Orphan processes | 维护 backend/PostgreSQL pid file，启动前清理同 app data dir 的旧进程 |
| Data corruption on app update | 数据目录永远在 App Support，不在 `.app` bundle 内 |
| Migration failure | backend 启动 health 暴露数据库状态，UI 显示诊断页 |
| Intel/Apple Silicon split | PR-6 先验证本机 host target，再扩展 universal build |
| Bundled Node 不完整或不可执行 | PR-6.4 smoke 显式执行 bundled Node 和 agent-worker readiness 检查 |

本机生成的 `0.7.0` ARM64 `.dmg` 为 93 MB，校验和与镜像资源检查通过。包内 PostgreSQL 已显式使用 bundled share 模板目录；在清空环境变量后，`initdb`、`postgres` 和 `createdb` 可仅依赖 App 内资源完成空库初始化。构建脚本同时处理 Apple Silicon `/opt/homebrew` 与 Intel `/usr/local` 依赖路径。发行 workflow 先保留 draft，全部架构资源校验通过后再公开。当前本地没有 Apple Developer 证书，因此该构建未完成正式签名和公证。Windows 暂不进入 `0.7.0` 发布矩阵。Windows packaged PostgreSQL runtime 和本地服务生命周期完成前，不生成已知无法独立启动的安装包。

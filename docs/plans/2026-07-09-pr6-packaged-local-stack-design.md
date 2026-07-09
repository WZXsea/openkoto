# PR-6 Packaged Local Stack Design
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
  E --> H["~/Library/Application Support/OpenKoto Desktop/postgres"]
  F --> I["~/Library/Application Support/OpenKoto Desktop/backend/files"]
```
## PR-6 Scope
PR-6 不改变 PR-4/PR-5 的 Backend-first 数据模型。桌面端仍通过 Axum Backend 访问 PostgreSQL；本 PR 只把这些服务纳入 `.app` 的本地生命周期管理。
## Subphase Plan
| Phase | Goal | User-visible result | Verification |
|---|---|---|---|
| PR-6.1 | Backend binary packaging and startup manager | `.app` 可以自动启动 bundled `openkoto-backend`，并把本地 backend URL 写入 desktop config | backend build script, Rust tests, desktop `cargo check/test`, packaged config check |
| PR-6.2 | Bundled PostgreSQL runtime | 首次启动自动 `initdb`，之后自动启动本地 PostgreSQL | PostgreSQL smoke test, migration smoke test, restart test |
| PR-6.3 | First-run local account and worker runtime | 用户不再手动填写 backend URL；agent-worker 不依赖系统 Node | Tauri smoke flow, bundled Node check |
| PR-6.4 | `.dmg` release smoke test | 构建出的 `.dmg` 安装后能创建文章、划词候选、重启后数据仍在 | install/open/smoke/restart script |
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
4. 自动分配 `127.0.0.1:<free-port>`。
5. 启动 backend resource binary 时注入：
   - `OPENKOTO_BACKEND_BIND`
   - `OPENKOTO_JWT_SECRET`
   - `OPENKOTO_FILE_STORAGE_DIR`
   - `DATABASE_URL`
6. 自动把 `backend_url` 写入 desktop `config.json`。
### Database boundary in PR-6.1
PR-6.1 不内置 PostgreSQL。数据库连接优先级：
1. `OPENKOTO_PACKAGED_DATABASE_URL`
2. `DATABASE_URL`
3. `postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev`
这个阶段用于证明 backend 生命周期和 packaging 通道；完整 `.dmg` 开箱即用要到 PR-6.2 才成立。
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
2. `DATABASE_URL`
3. `.app/Contents/Resources/postgres` 中的 bundled PostgreSQL runtime
4. 开发版 fallback：`postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev`
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
| `.dmg` 体积变大 | 先接受 PostgreSQL bundle 体积，后续再评估 SQLite desktop profile |
| macOS signing fails on nested binaries | 所有 sidecar 和 PostgreSQL binaries 进入 signing inventory |
| Port conflict | 服务管理器动态分配端口 |
| Orphan processes | 维护 pid file，启动前清理同 app data dir 的旧进程 |
| Data corruption on app update | 数据目录永远在 App Support，不在 `.app` bundle 内 |
| Migration failure | backend 启动 health 暴露数据库状态，UI 显示诊断页 |
| Intel/Apple Silicon split | PR-6 先验证本机 host target，再扩展 universal build |
| Agent worker still depends on host Node | PR-6.4 smoke test 必须显式检查；后续需要把 Node runtime 也纳入 bundle 或降级 agent worker 功能 |

# OpenKoto Backend

## 定位

`openkoto-backend` 是 PR-4.1 计划中的独立 Rust 后端服务目录，目标是承接 OpenKoto 从桌面本地 JSON 存储迁移到 Backend + PostgreSQL 架构的第一步。

本 worker 只提供 PR-4.1 的开发环境、启动约定和验收文档，不包含业务实现，也不修改现有 Desktop 代码。

## PR-4.1 技术边界

| 项目 | 选择 |
|---|---|
| HTTP 框架 | Axum |
| 异步运行时 | Tokio |
| 数据库 | PostgreSQL 16 |
| 数据库访问 | SQLx |
| 配置来源 | 环境变量 |
| 健康检查 | `GET /health` |
| 文件库根目录 | `OPENKOTO_FILE_STORAGE_DIR` |

PR-4.1 只允许新增 OpenKoto Backend 与 PostgreSQL 开发依赖。Anki、Zotero、MinerU、LanguageTool、MCP、浏览器插件仍不接入。

## 目录约定

```text
openkoto-backend/
├── README.md
├── VERIFICATION.md
├── .env.example
├── .gitignore
└── migrations/
    └── README.md
```

后续实现后端骨架时，建议保持以下结构：

```text
openkoto-backend/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── config.rs
│   ├── db.rs
│   ├── error.rs
│   └── routes/
│       └── health.rs
└── migrations/
    └── 0001_init.sql
```

## 环境变量

| 变量 | 示例 | 说明 |
|---|---|---|
| `DATABASE_URL` | `postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev` | SQLx PostgreSQL 连接串 |
| `OPENKOTO_BACKEND_BIND` | `127.0.0.1:4000` | 后端监听地址 |
| `OPENKOTO_JWT_SECRET` | `replace-with-a-local-development-secret-at-least-32-bytes` | PR-4.2 认证阶段使用 |
| `OPENKOTO_FILE_STORAGE_DIR` | `./.data/files` | PR-4.3 文件库根目录 |

初始化本地配置：

```bash
cp openkoto-backend/.env.example openkoto-backend/.env
```

## PostgreSQL 开发实例

根目录提供 `docker-compose.dev.yml`，默认把 PostgreSQL 容器端口映射到宿主机 `5433`，避免和本机已有 PostgreSQL 的 `5432` 冲突。

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
docker compose -f docker-compose.dev.yml ps
```

关闭数据库：

```bash
docker compose -f docker-compose.dev.yml down
```

清空开发数据库数据：

```bash
docker compose -f docker-compose.dev.yml down -v
```

## 后端骨架启动约定

后续完成 Rust skeleton 后，建议从仓库根目录运行：

```bash
set -a
. openkoto-backend/.env
set +a

cargo run --manifest-path openkoto-backend/Cargo.toml
```

健康检查：

```bash
curl -fsS http://127.0.0.1:4000/health
```

建议 `GET /health` 返回：

```json
{
  "status": "ok",
  "service": "openkoto-backend",
  "database": "ok"
}
```

## 错误响应约定

PR-4.1 建议先固定统一错误响应格式，供后续认证和业务 API 复用：

```json
{
  "error": {
    "code": "database_unavailable",
    "message": "Database is unavailable",
    "request_id": "optional-request-id"
  }
}
```

## 验证入口

查看完整验收说明：

```bash
sed -n '1,220p' openkoto-backend/VERIFICATION.md
```

运行环境检查脚本：

```bash
bash script/verify_pr4_1_backend_env.sh
```

需要同时启动 PostgreSQL 时：

```bash
bash script/verify_pr4_1_backend_env.sh --start-db
```

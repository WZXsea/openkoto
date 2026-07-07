# OpenKoto Backend

`openkoto-backend` 是 OpenKoto Desktop + Backend + PostgreSQL 架构中的 Axum 后端服务。PR-4.2 在 PR-4.1 后端骨架上补充 PostgreSQL 用户账户、登录会话和 Bearer JWT 认证。

## 范围

PR-4.2 提供：

- Rust Axum HTTP service。
- SQLx PostgreSQL connection pool。
- 启动时执行 SQLx migrations。
- PostgreSQL `users` / `sessions` 表。
- Argon2id 密码哈希，不保存明文密码。
- Bearer JWT 认证，签名密钥来自 `OPENKOTO_JWT_SECRET`。
- `GET /health`。
- `POST /auth/register`。
- `POST /auth/login`。
- `GET /auth/me`。
- 统一错误响应格式。
- `OPENKOTO_TEST_DATABASE_URL` 控制的可选数据库集成测试。

PR-4.2 不切换 Desktop 数据读写路径。现有 Tauri commands 和本地 JSON storage 仍保留到后续 PR-4 阶段。

## 本地开发

从仓库根目录启动 PostgreSQL：

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
```

准备本地配置并启动后端：

```bash
cp openkoto-backend/.env.example openkoto-backend/.env
cargo run --manifest-path openkoto-backend/Cargo.toml
```

后端优先加载当前目录下的 `.env`，随后回退到 `openkoto-backend/.env`。

## 配置

| 变量 | 默认值 | 用途 |
|---|---|---|
| `DATABASE_URL` | `postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev` | 运行时 PostgreSQL 连接串。后端启动和 migration 需要可连接数据库。 |
| `OPENKOTO_TEST_DATABASE_URL` | unset | 可选测试数据库连接串。未设置时，`cargo test` 跳过 PostgreSQL 集成测试，只运行不依赖数据库的测试。 |
| `OPENKOTO_BACKEND_BIND` | `127.0.0.1:4000` | Axum HTTP 监听地址。 |
| `OPENKOTO_JWT_SECRET` | `openkoto-dev-insecure-change-me` | JWT 签名密钥。认证接口要求该值不是默认值，且长度至少 32 bytes。 |
| `OPENKOTO_FILE_STORAGE_DIR` | `.data/files` | 后续文件存储目录。 |

## 健康检查

```bash
curl -fsS http://127.0.0.1:4000/health
```

响应外形：

```json
{
  "status": "ok",
  "service": "openkoto-backend",
  "version": "0.1.0",
  "database": {
    "connected": true,
    "latency_ms": 1
  },
  "file_storage": {
    "ready": true
  },
  "auth": {
    "configured": true
  },
  "started_at": "2026-07-07T00:00:00Z"
}
```

## 认证接口

所有认证接口使用 JSON request/response。登录后客户端通过 `Authorization: Bearer <jwt>` 访问需要认证的接口。

### 注册

```bash
curl -i -X POST http://127.0.0.1:4000/auth/register \
  -H 'Content-Type: application/json' \
  -d '{
    "email": "reader@example.com",
    "password": "correct-horse-battery-staple",
    "display_name": "Reader"
  }'
```

成功响应示例：

```json
{
  "user": {
    "id": "018f4e7a-7b49-7b52-9f6f-682dbdbe9f42",
    "email": "reader@example.com",
    "display_name": "Reader",
    "created_at": "2026-07-07T12:00:00Z",
    "updated_at": "2026-07-07T12:00:00Z"
  },
  "token": "<jwt>",
  "token_type": "bearer",
  "expires_at": "2026-08-06T12:00:00Z"
}
```

### 登录

```bash
curl -i -X POST http://127.0.0.1:4000/auth/login \
  -H 'Content-Type: application/json' \
  -d '{
    "email": "reader@example.com",
    "password": "correct-horse-battery-staple"
  }'
```

成功响应与注册接口一致，返回当前用户和新的 Bearer JWT。

### 当前用户

```bash
TOKEN="$(curl -fsS -X POST http://127.0.0.1:4000/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"reader@example.com","password":"correct-horse-battery-staple"}' \
  | jq -r '.token')"

curl -i http://127.0.0.1:4000/auth/me \
  -H "Authorization: Bearer ${TOKEN}"
```

成功响应示例：

```json
{
  "user": {
    "id": "018f4e7a-7b49-7b52-9f6f-682dbdbe9f42",
    "email": "reader@example.com",
    "display_name": "Reader",
    "created_at": "2026-07-07T12:00:00Z",
    "updated_at": "2026-07-07T12:00:00Z"
  }
}
```

## 错误响应

后端错误统一返回以下外形：

```json
{
  "error": {
    "code": "invalid_credentials",
    "message": "invalid email or password"
  }
}
```

常见认证错误包括：

| HTTP Status | 示例 code | 含义 |
|---|---|---|
| `400` | `invalid_email` / `weak_password` | 请求字段格式无效。 |
| `401` | `invalid_credentials` | 登录邮箱或密码错误。 |
| `401` | `missing_bearer_token` / `invalid_token` | 缺少 Bearer token、token 无效或 token 已失效。 |
| `409` | `email_already_registered` | 注册邮箱已存在。 |

## 验证

```bash
bash script/verify_pr4_2_backend_auth.sh
cargo check --manifest-path openkoto-backend/Cargo.toml
cargo test --manifest-path openkoto-backend/Cargo.toml
```

普通 `cargo test` 在未设置 `OPENKOTO_TEST_DATABASE_URL` 时会跳过 PostgreSQL 集成测试。需要运行 DB 集成测试时，先启动 PostgreSQL，再显式传入测试数据库连接串：

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
OPENKOTO_TEST_DATABASE_URL=postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev \
  cargo test --manifest-path openkoto-backend/Cargo.toml
```

需要 verification script 代为启动开发数据库时使用 `--start-db`：

```bash
bash script/verify_pr4_2_backend_auth.sh --start-db
```

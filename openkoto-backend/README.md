# OpenKoto Backend

`openkoto-backend` is the PR-4.1 Rust backend skeleton for the OpenKoto Desktop + Backend + PostgreSQL architecture.

## Scope

PR-4.1 provides:

- Rust Axum HTTP service.
- SQLx PostgreSQL connection pool.
- SQLx migrations executed on startup.
- `GET /health`.
- Shared error response shape.
- Development PostgreSQL compose file.

PR-4.1 does not switch Desktop data access yet. Existing Tauri commands and JSON storage remain unchanged until later PR-4 stages.

## Development

Start PostgreSQL from the repository root:

```bash
docker compose -f docker-compose.dev.yml up -d openkoto-postgres
```

Run the backend:

```bash
cp openkoto-backend/.env.example openkoto-backend/.env
cargo run --manifest-path openkoto-backend/Cargo.toml
```

The backend loads `.env` from the current directory first, then falls back to `openkoto-backend/.env`.

## Configuration

| Variable | Default |
|---|---|
| `DATABASE_URL` | `postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev` |
| `OPENKOTO_BACKEND_BIND` | `127.0.0.1:4000` |
| `OPENKOTO_JWT_SECRET` | `openkoto-dev-insecure-change-me` |
| `OPENKOTO_FILE_STORAGE_DIR` | `.data/files` |

## Health Check

```bash
curl -fsS http://127.0.0.1:4000/health
```

Response shape:

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

## Verification

```bash
bash script/verify_pr4_1_backend_env.sh
cargo check --manifest-path openkoto-backend/Cargo.toml
cargo test --manifest-path openkoto-backend/Cargo.toml
```

Use `--start-db` when the PostgreSQL development container should be started by the verification script:

```bash
bash script/verify_pr4_1_backend_env.sh --start-db
```

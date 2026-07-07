# Migrations

本目录存放 `openkoto-backend` 的 SQLx PostgreSQL migrations。

命名约定：

```text
YYYYMMDDHHMMSS_short_description.sql
```

## 当前 schema 阶段

| 阶段 | 用途 |
|---|---|
| PR-4.1 | Backend bootstrap migration。创建 `backend_metadata`，SQLx 维护 `_sqlx_migrations`。 |
| PR-4.2 | Authentication migration。增加 `users` 和 `sessions`，用于本地账户注册、登录和 Bearer JWT session tracking。 |

## PR-4.2 认证表

`users` 保存持久账户记录：

- 稳定 user id。
- 唯一 email。
- 可选 display name。
- Argon2id password hash。
- 创建和更新时间戳。

`sessions` 保存登录会话和 JWT metadata：

- 稳定 session id。
- 指向 `users` 的 `user_id` foreign key。
- 不可逆的 `token_hash`。
- session 创建和过期时间戳。
- `revoked_at` 和 `last_used_at` metadata。

Migrations 不得保存明文密码或明文 Bearer token。密码只以 Argon2id hash 表示；token 持久化只应保存 metadata 或不可逆 token identifier。

## 测试数据库

`OPENKOTO_TEST_DATABASE_URL` 只用于可选 PostgreSQL integration tests。未设置该变量时，`cargo test --manifest-path openkoto-backend/Cargo.toml` 应跳过 DB integration tests，使本地无 PostgreSQL 时仍可运行普通测试。

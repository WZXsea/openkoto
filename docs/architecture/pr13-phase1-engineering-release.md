## PR-13 第一阶段工程封版

### 目标

PR-13 不扩大产品功能范围，它将 PR-7～PR-12 已交付的素材、批注、学习对象、首页和 Assistant 收敛为可维护、可恢复、可重复验证和可发行的第一阶段独立工作台。本阶段仍不连接 Anki、Zotero、MinerU、LanguageTool、MCP 或其他外部软件。

### 工程边界

| 领域 | PR-13 收敛 | 兼容要求 |
|---|---|---|
| Desktop command | 将配置、认证和模型配置迁入独立领域模块 | Tauri command 名、参数和返回 DTO 不变 |
| Reader | 拆出顶部控制和 Assistant 外壳，保留单一阅读状态所有者 | 阅读、进度、书签、批注、候选项和来源返回行为不变 |
| Settings | 按账户、模型、界面和运行诊断拆分面板 | 保存、取消、预览和重启保持契约不变 |
| Packaged runtime | 增加升级备份校验、原子指纹提交、迁移失败诊断和端口占用说明 | 不覆盖不完整 PostgreSQL 目录，不终止未经精确匹配的进程 |
| Legacy import | 将源数量、失败数、分类计数和源摘要固定到导入报告 | 校验 client ID、服务端摘要格式、回显报告、计数和逐项结果 |
| Release | 统一 Backend、Desktop、worker、Frontend、Playwright 和打包门禁 | 发行产物必须对应确定提交和一致版本 |

### 数据与升级保证

1. 升级备份格式升为 v2，备份 PostgreSQL、配置、JWT 和 Backend 文件存储的实际字节。
2. `manifest.json` 和文件条目保存 SHA-256，并在 dry restore 中重新校验。
3. 已存在但不完整的 PostgreSQL 数据目录不会被 `initdb` 重新初始化。
4. Backend 健康检查成功后才会原子提交新二进制指纹；失败信息保留备份位置和 dry restore 路径。
5. 端口占用只生成诊断；清理进程要求 PID 文件和本应用二进制路径精确匹配，PostgreSQL 另外要求本应用数据目录参数匹配。

### 发行等级

| 等级 | 用途 | 必须门禁 | 发布状态 |
|---|---|---|---|
| PR-13 工程候选版 | 功能审计、交互调整和本机升级 | 全量测试、历史回归、packaged smoke、备份与恢复校验、ad-hoc 包体校验 | GitHub prerelease，不声明 Apple 公证 |
| 第一阶段正式发行 | 面向普通 macOS 用户分发 | 上述全部门禁 + Developer ID 签名 + notarization + staple + `spctl` | 仅在 Apple 凭据和双架构产物都通过后发布 |

### 正式发行流水线

1. `ci-gate` 在独立 PostgreSQL 16 服务中执行 Backend 格式、编译和测试。
2. agent-worker 执行 typecheck、test 和 build；Frontend 执行 typecheck、生产构建、覆盖率和 Playwright。
3. Desktop Rust 执行 fmt、check 和全 feature/全 target 测试。
4. macOS ARM64 和 x86_64 构建任务在签名前检查全部 Apple 凭据，缺失时只报告 secret 名称。
5. 发行草稿中的 `.app` 和 `.dmg` 必须通过资源完整性、`codesign --verify`、`spctl`、staple 和 DMG 校验，随后才能转为已发布状态。

### 凭据边界

正式 macOS 发行需要 `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`、`APPLE_API_KEY_CONTENT`、`APPLE_API_ISSUER`、`APPLE_API_KEY`、`APPLE_TEAM_ID` 和 `KEYCHAIN_PASSWORD` 仓库 secret。本机和 GitHub 仓库不存在这些可用凭据时，PR-13 可交付经验证的工程候选版，但正式发行门禁保持未通过状态。

### 验收门禁

- [x] command 名和 DTO 契约回归通过。
- [x] Reader、Settings、offline、error 和任务恢复状态回归通过。
- [x] 备份篡改、dry restore、不完整 PostgreSQL 目录、升级失败和 legacy 报告测试通过。
- [x] Backend、Desktop、agent-worker、Frontend、Playwright 和 PR-7～PR-12 历史回归通过。
- [ ] packaged runtime、安装、覆盖升级、重启和数据保持 smoke 通过。
- [ ] Developer ID 签名、公证、staple 和 Gatekeeper 评估通过。

### 验证记录

1. `bash script/verify_pr13_phase1_release.sh --full --start-db` 在 0.11.0 契约下通过 Backend 全量数据库测试、agent-worker 37 项测试、Desktop Rust 103 项库测试与全部集成测试、Frontend 232 项测试、生产构建和 Playwright。
2. Playwright 结果为 5 项通过、2 项按环境条件跳过。
3. `bash script/verify_pr13_phase1_release.sh --history` 在 0.11.0 契约下通过 PR-7～PR-12 历史回归链。
4. 发行工作流 YAML、结构检查和 9 项单元测试通过。

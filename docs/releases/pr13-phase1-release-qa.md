## PR-13 第一阶段封版 QA
### 版本策略
工程候选版本由主线在全量验证完成后统一更新。验证时使用 `${EXPECTED_VERSION}`；未显式传入时，`script/verify_pr13_phase1_release.sh` 从 `textlingo-desktop/src-tauri/Cargo.toml` 读取当前版本，并调用统一版本检查脚本核对 Backend、Desktop、前端包和 Tauri 配置。

### 封版入口
| 层级 | 命令 | 覆盖范围 |
| --- | --- | --- |
| 静态门禁 | `bash script/verify_pr13_phase1_release.sh` | Shell 语法、Rust 格式、版本一致性、运行时安全锚点、备份/恢复锚点、legacy import 报告、diff 空白错误 |
| 完整验证 | `bash script/verify_pr13_phase1_release.sh --full --start-db` | 隔离 PostgreSQL、Backend、agent worker、Desktop Rust、前端测试与构建、Playwright |
| 历史回归 | `bash script/verify_pr13_phase1_release.sh --history` | PR-12 → PR-11 → PR-10 及更早静态/回归门禁 |
| 完整加历史 | `bash script/verify_pr13_phase1_release.sh --full --start-db --history` | 完整验证并串联历史门禁 |
| 本地打包冒烟 | `bash script/verify_pr13_phase1_release.sh --package-smoke` | Backend sidecar、PostgreSQL、Node runtime 和本地 DMG 构建 |

### 数据升级保护验收
1. Backend 指纹变化且存在受管 PostgreSQL 数据时，升级前生成 `backend/backups/pre-upgrade-*`。
2. 备份格式复制停机后的 `postgres-data`、`backend/files`，并按存在情况复制 `config.json`、`jwt_secret`。
3. `manifest.json` 保存所有快照文件的相对路径、大小和 SHA-256；`manifest.sha256` 校验清单本身。
4. staging 备份只有在清单验证和 dry restore 均通过后才能原子提交；失败时不写入新 Backend 指纹。
5. Backend 提前退出、migration/health 失败、数据库目录不完整时，诊断必须说明旧数据未删除、指纹未提交、可用备份路径和恢复前置条件。
6. 不完整的 PostgreSQL 数据目录不得被重新 `initdb` 覆盖；首次初始化使用 staging 目录并在成功后重命名提交。

### 端口与进程安全验收
1. 固定 Backend 端口被占用时只输出监听进程摘要和处理建议，不从端口冲突路径执行 `kill`。
2. Backend 残留进程只有在 PID 文件存在且进程命令以精确、bounded 的本应用 Backend binary 开头时才允许清理；不匹配时拒绝终止。
3. PostgreSQL 残留进程只有在 PID、精确可执行文件边界和应用专属 data directory 同时匹配时才允许清理。
4. 命令匹配必须拒绝可执行文件前缀伪装和 data directory 前缀伪装。

### Legacy import 报告验收
1. 导入前生成 `source_report`，记录 schema、源内容 SHA-256、总数、采集失败数和分类型计数。
2. `client_import_id` 基于已脱敏请求和源报告稳定生成，相同输入重复执行保持一致。
3. Backend 返回后核对 client import ID、Backend 请求校验和格式、schema、总数、计数守恒、逐项结果数量和回显的源报告。
4. 任一核对失败均返回“保留 source JSON”的恢复提示，不删除 legacy 原始文件。

Desktop 不直接把本地请求重新序列化所得 SHA-256 与 Backend `request_sha256` 比较。含 segments 的材料会在 Backend 反序列化为受控 `MaterialSegmentInput`，字段集合与顺序经过规范化，因而跨端再次序列化不保证字节一致。当前端到端证据由稳定 `client_import_id`、服务端 hash 格式、`source_report` 精确回显、计数守恒和逐项结果共同构成；若后续要求单一端到端 checksum，应由 Backend 增加明确的 source payload checksum 契约。

### 人工封版检查
1. 在旧版本真实 App Support 副本上执行升级，确认 migration 成功后材料、文件、学习记录和 Assistant 历史可读。
2. 使用故意失败的 Backend/migration 候选包验证诊断信息和备份路径，禁止直接覆盖真实用户目录。
3. 对备份副本执行校验和 dry restore，抽查 PostgreSQL `PG_VERSION`、代表性 `backend/files`、配置和 JWT secret 权限。
4. 使用独立监听程序占用 `127.0.0.1:19421`，确认应用报告冲突且监听程序仍存活。
5. 执行 legacy import 后核对 batch 总数、失败项、`source_report` 和代表性目标记录，核对完成前保留原始 JSON。

### Apple 发布边界
本地脚本可以完成未签名或本地签名的构建与 DMG 冒烟，不能替代正式 macOS 发布验证。以下步骤仍依赖主线持有的 Apple Developer 凭据和发布环境：Developer ID Application 签名、Developer ID Installer 签名（如使用）、Hardened Runtime/entitlements 验证、Apple notarization、stapling，以及在干净 Mac 上执行 Gatekeeper `spctl` 验证。缺少这些凭据时，工程验证结果只能标记为“代码与本地包候选通过”，不能标记为“可公开分发已验证”。

### 0.11.0 本机候选验收记录
1. `bash script/verify_pr13_phase1_release.sh --package-smoke` 通过，生成 ARM64 `.app` 和 `.dmg`。
2. 以 `signingIdentity: "-"` 生成 ad-hoc hardened runtime 候选包；应用和 DMG 盘内应用均通过 `codesign --verify --deep --strict`，DMG 通过 `hdiutil verify`。
3. 0.10.0 应用和 App Support 在进程停止、PostgreSQL 正常关闭后完成离线备份；代表性素材、Assistant checkpoint 和收藏文件哈希与升级前一致。
4. 0.11.0 首次启动生成 v2 pre-upgrade backup；清单本身 SHA-256 一致，1,467 个 snapshot entry 的路径、大小和 SHA-256 全部验证通过。
5. 安装后 Backend 报告 0.11.0，PostgreSQL connected、file storage ready、auth configured；主进程重启后冷启动进入首页。
6. 实机覆盖首页、84 天热力图、素材筛选与 Reader 返回、Reader 沉浸模式、学习工作台、Assistant 时间线和拆分后的设置面板。

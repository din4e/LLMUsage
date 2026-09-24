# 代码审计报告 · 2026-09-24

- **范围**：`src/`（全部 TS 源码与测试）、`src-tauri/`（全部 Rust 源码、`Cargo.toml`、`tauri.conf.json`、capabilities），共约 11,000 行。`dist/`、`gen/schemas`、`package-lock.json` 未审。
- **方法**：逐文件人工通读 + 交叉核对前后端契约；运行 `npm test`（76 个用例全部通过）与 `cargo check`（通过）。
- **基线**：dev 分支 `03ccaa0` + 工作区未提交的 `src/app.ts` 状态栏改动。
- **总体评价**：代码质量高于同类个人项目平均水平。安全设计尤其扎实（DPAPI/Keychain、路径穿越防护、敏感头标记、zeroize、不可信 JSON 的严格校验与上限、CSP、`https_only`、导入文件大小/数量上限）。主要风险集中在**本地状态文件的错误恢复**与**跨平台缺口**两块。

## 发现索引

| # | 严重度 | 摘要 | 位置 |
|---|--------|------|------|
| 1 | 高 | history 文件损坏/超限后所有同步永久失败，且报错误导为「凭据管理器」 | `cache.rs` / `app.rs` |
| 2 | 高 | 负余额可触发同样的永久同步失败循环（解析器与记录校验不对称） | `online.rs` / `cache.rs` |
| 3 | 高 | Linux 构建必然编译失败：`keyring` 仅在 macOS target 声明 | `Cargo.toml` / `secret.rs` |
| 4 | 高 | macOS 上实例枚举只扫 `.dpapi` 文件，Keychain 凭据永远列不出来 | `transfer.rs` |
| 5 | 中 | 单个缓存文件损坏 → 全部供应商缓存不可见，无自愈 | `cache.rs` / `app.rs` |
| 6 | 中 | 明细卡「重置星期」取自当前时间而非重置时间 | `domain.ts` |
| 7 | 中 | 在线供应商行的冷却倒计时永不刷新（只刷新 GLM） | `app.ts` |
| 8 | 中 | `has_more` / `next_page` 分页被忽略，大组织数据会静默低估 | `online.rs` |
| 9 | 中 | 完整备份明文密钥文件未收紧权限（Unix 下 0644） | `app.rs` |
| 10 | 中 | 汇率请求 frankfurter.app 与「仅访问供应商域名」的隐私声明不符，且无缓存 | `online.rs` / `index.html` |
| 11 | 中 | 合计余额趋势永久计入已删除实例，与「今日消耗」口径不一致 | `domain.ts` / `app.ts` |
| 12 | 中(待核) | Kimi Global（api.moonshot.ai）余额按 CNY 展示，国际站或为 USD | `online.rs` |
| 13 | 低 | GLM「今日」窗口用本机时区拼时间串，非 UTC+8 用户与 GLM 计费日错位 | `domain.ts` |
| 14 | 低 | 时间范围非法被映射成各供应商的「密钥问题」文案 | `app.rs` |
| 15 | 低 | history 写入 remove+rename 非原子且无 fsync，崩溃可整文件丢失 | `cache.rs` |
| 16 | 低 | GLM 无重置时间的行，30 秒后 quota-hint 变成空白 | `app.ts` |
| 17 | 低 | 拖拽句柄 mouseup 只监听句柄自身，异常释放后整行保持可拖 | `app.ts` |
| 18 | 低 | GLM 同步失败消息不带实例名前缀（online 侧带），多实例无法区分 | `app.ts` |
| 19 | 低 | 主题选择不持久化，每次重启回浅色 | `app.ts` |
| 20 | 低 | `domain.ts credentialHint()` 为死代码（逻辑已迁入 providers.ts） | `domain.ts` |
| 21 | 低 | index.html 静态初值过时（coverage "0 / 9"、v0.1.5） | `index.html` |
| 22 | 低 | 杂项：click 分支缺 return、`applyAutoSync(NaN)`、导入 TOCTOU | `app.ts` / `app.rs` |

---

## 高严重度

### 1. history 文件损坏或超限 → 所有同步永久失败，且错误指向凭据管理器

**位置**：`src-tauri/src/cache.rs:76`（`upsert` 内 `self.load()?`）、`cache.rs:101-117`（`load` 对损坏 JSON / >8MB 返回 `Err`）、`src-tauri/src/app.rs:202-209`（`record_daily_usage` 把一切 `CacheError` 映射为 `CommandError::credential()`）。

**现象**：`daily-usage.json` 一旦反序列化失败（损坏）或超过 `MAX_HISTORY_BYTES`（8 MiB），或含任何一条不合法记录（见 #2）：

1. `load_daily_usage` 静默返回空数组（`unwrap_or_default`），趋势图/最近变化/今日消耗全部消失，无提示；
2. 之后**每一次**同步：网络请求与快照缓存都已成功，但 `record_daily_usage → upsert → load()?` 失败 → 整个 `sync_*` 命令返回 `CREDENTIAL_ERROR`，前端把行标记为「同步失败」并显示 **「无法访问 Windows 凭据管理器」**；
3. 代码中没有任何路径重置、截断或备份该文件 → 故障**永久**，用户只能手工删文件。

**建议**：
- `record_daily_usage` 用独立的错误码/文案（「本地用量历史写入失败」），与凭据存储解耦；
- `upsert` 在 `load` 失败时降级为「以新记录重新开始」，把坏文件改名为 `.corrupt` 留档；
- `load_daily_usage` 失败时在状态栏提示，而不是静默空白。

### 2. 负余额触发同样的失败循环：解析器允许负数，记录校验拒绝负数

**位置**：`src-tauri/src/cache.rs:194-206`（`is_valid_daily_record` 要求 `balance_cny >= 0`）；对照 `online.rs:2252-2258`（`parse_money` 只检查 `is_finite`）及 `parse_kimi` / `parse_deepseek` / `parse_siliconflow` / `parse_ppio`（均不拒绝负余额）。

**现象**：任一余额型供应商返回负的可用余额（PPIO 有 `pendingCharges` 信用额度场景、xAI 预付余额理论上可透支为负），快照解析成功、界面正常显示，但当日采样写入被 `Invalid` 拒绝 → 该实例每次同步都在 `record_daily_usage` 步骤失败并被标记「同步失败 · 凭据管理器」（同 #1 的误导文案）。OpenRouter 已有 `total_credits >= total_usage` 守卫，其余家没有。

**建议**：两端口径取其一 —— 解析侧拒绝负余额（`SchemaMismatch`），或记录侧允许负数（余额本就是存量，透支是真实状态）。推荐后者并保留 `is_finite` 校验。

### 3. Linux 构建必然失败：`keyring` 依赖只声明在 macOS target

**位置**：`src-tauri/Cargo.toml:32-34`（`keyring = "3"` 仅在 `cfg(target_os = "macos")`）；`src-tauri/src/secret.rs:150-153、164-171`（`#[cfg(not(target_os = "windows"))]` 下直接引用 `keyring::Entry` / `keyring::Error`）。

**现象**：在 Linux 上 `cfg(not(windows))` 为真，但 `keyring` 不是依赖 → unresolved import，编译失败。而 `main.rs:22-25`、README、tauri.conf（`targets: "all"`）都保留了 Linux 自启动/打包的意图。

**建议**：把 keyring 依赖改为 `[target.'cfg(not(target_os = "windows"))'.dependencies]`，或把 secret.rs 的非 Windows 分支一并收敛到 `cfg(target_os = "macos")` 并给 Linux 一个显式编译错误提示。静态分析结论（本机为 Windows，未实际交叉编译验证）。

### 4. macOS：实例枚举只扫 `.dpapi` 文件，Keychain 凭据永远列不出来

**位置**：`src-tauri/src/transfer.rs:131-156`（`enumerate_instances` 只读 `credentials/*.dpapi`）；对照 `secret.rs:93-101`（非 Windows 平台凭据存 Keychain，不落盘）。

**现象**：`list_provider_instances`、`export_provider_backup`、`apply_import` 的 existing 集合全部依赖 `enumerate_instances`。在 macOS 上即使凭据保存成功，重启后 `list_provider_instances` 返回空 → 已配置实例全部从界面消失；导出（full/status）只会包含 0 个实例；导入的防冲突后缀也失效。README 已声明「macOS/Linux 仍需补齐」，但这是补齐时容易漏掉的具体断点清单：**枚举/导出/导入判重三条链路都只认 dpapi 文件**。

**建议**：macOS 侧需要一个 keychain 枚举来源（例如在 app data 里另存一份「已配置实例 id 清单」文件，写入/删除与 vault 同步），或维护一个 instance registry。

---

## 中严重度

### 5. 单个缓存文件损坏 → 全部供应商缓存不可见

**位置**：`src-tauri/src/cache.rs:148-165`（`load_all` 遇到任一文件解析失败整体返回 `Err`）；`app.rs:333-337`（`unwrap_or_default`）。

**现象**：cache 目录里一个坏 JSON（例如崩溃遗留的半截文件——见 #15 的非原子写）会让启动时**所有**供应商的缓存快照消失，直接掉到「等待同步」空态，且没有修复路径。`load_all` 应当跳过坏文件继续读其余的，必要时顺带删除或改名坏文件。

### 6. 明细卡「重置星期」取自当前时间，不是重置时间

**位置**：`src/domain.ts:480-488`：

```ts
const dayIndex = new Date(nowMs).getDay();          // ← 今天
return `${WEEKDAY_LABELS[dayIndex]} · 剩余 ${formatDuration(remainingMs)}`;
```

**现象**：标签想表达「重置发生在周几」，但取的是 `nowMs` 的星期。重置在明天时，用户看到「周三 · 剩余 14 小时」而实际重置是周四。`domain.test.ts:478-485` 用 now=周三、reset=下周一的用例把该行为固化成了预期（断言含「周三」）。

**建议**：改为 `new Date(resetAtMs).getDay()`，同步修正测试；若剩余超过 24h 也可以顺带显示日期。

### 7. 在线供应商行的冷却倒计时永不刷新

**位置**：`src/app.ts:836-840`（`renderOnline` 仅在同步/缓存加载时写一次 `quota-hint`）；`app.ts:893-909`（`updateCooldown` 每 30 秒只遍历 `glmSnapshots`）。

**现象**：在线行显示「5 小时后恢复」后，倒计时静止不动，冷却到期后仍停留在旧文案，直到下一次同步。GLM 行有 30 秒刷新，行为不一致。

**建议**：把 online 快照的 `cooldownEndsAtMs` 纳入 `updateCooldown`（注意保留 `failedSyncInstances` 跳过逻辑与 `quotaUsedPercent == null` 的余额型行）。

### 8. `has_more` / `next_page` 分页被忽略 → 大组织数据静默低估

**位置**：`src-tauri/src/providers/online.rs`：
- OpenAI 组织用量/成本：`limit=31`（`:591、:599`），响应中的 `has_more` 未读；
- Claude Code 日汇总：`limit=1000`（`:615`），`has_more` / `next_page` 未读；
- Anthropic Messages：`limit=31`（`:640`），同上。

**现象**：多成员组织或模型数超限的账户会拿到截断的第一页，Token/成本被低估且无任何提示。单日窗口通常触发不了，但 Claude Code 的 1000 条按 (日期×成员) 展开时中大型组织是能触发的。

**建议**：至少检测 `has_more == true` 时在 `secondary_value` 标注「数据不完整」；理想情况跟随 `next_page` 翻页（有上限）。

### 9. 完整备份明文密钥文件未收紧权限

**位置**：`src-tauri/src/app.rs:626-627`（`std::fs::write(&path, json)`）。

**现象**：full 模式导出的 JSON 含全部明文 API Key。Unix 下 `fs::write` 以默认 umask 创建（通常 0644），同机其他本地用户可读。Windows 依赖目录 ACL，问题不大；macOS/Linux 打包后这就是实际风险（与 #3/#4 的平台补齐相关）。

**建议**：非 Windows 平台写入后 `set_permissions(0o600)`（`std::os::unix::fs::PermissionsExt`）。

### 10. 汇率请求与隐私声明不符，且每次同步都打第三方

**位置**：`src-tauri/src/providers/online.rs:806-823`（`fetch_usd_cny_rate` 请求 `api.frankfurter.app`，OpenAI / Claude Code 每次同步都调用，无缓存）；`index.html:209`（关于页声明「所有网络请求仅指向你配置的供应商域名，可在开发者工具中核对」）。

**现象**：两处问题——(a) 隐私声明与实现不符，用户在开发者工具里会看到非供应商域名的请求；(b) 自动同步（最短 1 分钟间隔）下会对 frankfurter.app 产生持续的外部请求，可能被限流，限流后 CNY 换算静默变 `None`。

**建议**：汇率结果缓存（例如 12–24 小时，进程内或落在 cache 目录）；同时修正关于页文案，注明「成本换算会访问 frankfurter.app 汇率服务」。

### 11. 合计余额趋势永久计入已删除实例

**位置**：`src/domain.ts:292-346`（`selectBalanceTrend` 对 "all" 把出现过的所有实例余额向前携带并求和，无配置过滤）；对照 `src/app.ts:355-364`（`renderTodaySpend` 特意只汇总 `configuredInstanceIds`，注释明说删除的账号要退出聚合）。

**现象**：删除一个账号后，「合计余额」曲线仍把它的最后已知余额永远计入；而「今日消耗」磁贴不再计入。两处口径不一致，用户对比曲线和磁贴会得到对不上的数字。趋势 Token 图同样含已删除实例（README 说这是有意的——历史保留），但余额是「当前存量」语义，与 Token「历史流量」不同，不该继续携带。

**建议**：`selectBalanceTrend` 增加可选的存活实例过滤（与 `renderTodaySpend` 同源），或至少在曲线说明里注明「含已删除实例的最后余额」。

### 12.（待人工核实）Kimi Global 余额按 CNY 展示

**位置**：`src-tauri/src/providers/online.rs:941-949`（`parse_kimi` 对 `KimiGlobal` 也走 `balance_snapshot(provider, available_balance, "CNY", …)`，`balance_cny` 计入人民币合计）。

**现象**：`api.moonshot.ai` 是国际站，计费通常以 USD 结算；响应体不含币种字段。若实际是 USD，则该行显示 `¥xx.xx` 是错的货币，还会被 `renderTotals` 当人民币余额汇总。无法离线确证，建议拿一个国际站账号核对一次；若为 USD，参照 OpenRouter 的 `"USD"` 分支处理（并且 `今日消耗` 的余额差分估算也需要同币种口径）。

---

## 低严重度

### 13. GLM「今日」窗口时区错位

`src/domain.ts:97-103` 用**本机时区**拼 `YYYY-MM-DD HH:MM:00` 传给 GLM（GLM 后端按北京时间解释）。非 UTC+8 用户的「今日 Token」与 GLM 计费日不一致。当前用户群以国内为主，影响有限；建议在代码注释或文档中标注该假设，或改用北京时间构造窗口。

### 14. 时间范围非法 → 报「密钥」错误

`src/app.rs:445-446、494-495`：`OnlineUsageRange::new` 的任何失败（本质是前端传参问题）都映射为 `InvalidCredential`，再经 `online_error` 变成「请检查 Google Cloud Project ID / Management Key…」等密钥排查文案。实际与密钥无关。建议 range 校验失败单独报错。

### 15. 本地 JSON 写入非原子、无 fsync

`src-tauri/src/cache.rs:93-98、140-145`：Windows 上 `rename` 不能覆盖已有文件，所以采用 `remove_file` + `rename`——两者之间崩溃会**整文件丢失**（tmp 还在，但没有恢复逻辑）；且 `write` 后没有 fsync，断电可能产生空文件。建议：启动或写入失败时尝试从同名 `.tmp` 恢复；关键文件（凭据、history）写后 fsync。坏档后果分别见 #1（history）与 #5（cache）。

### 16. GLM 无重置时间的行，hint 在 30 秒后变成空白

`src/app.ts:903-905`：`updateCooldown` 对 `cooldownEndsAtMs == 0` 的 GLM 行把 `quota-hint` 写成 `""`，覆盖了初始的「重置时间未知」，行内留下空洞。应保留原文案或显示「重置时间未知」。

### 17. 拖拽句柄异常释放后整行保持可拖

`src/app.ts:532-537`：`mouseup`/`pointercancel` 只挂在句柄上。在句柄按下、移出行外释放时 `row.draggable` 仍为 true，此后点击行内任意处按下即可拖动整行。建议改用 `setPointerCapture`，或在 `document` 上兜底 `mouseup` 释放。

### 18. GLM 同步失败消息不带实例名

`src/app.ts:950-954`（`syncGlm` 直接 `error.message`）对照 `:971-975`（`syncOnline` 走 `instanceError` 加「名称：」前缀）。多个 GLM 实例时状态栏无法区分是哪个实例失败。统一走 `instanceError` 即可。

### 19. 主题不持久化

`src/app.ts:1186-1189`：`data-light` 只在内存切换，重启回浅色（`index.html:2` 默认 `data-light`）。一行 localStorage 即可解决。

### 20. 死代码：`domain.ts credentialHint()`

`src/domain.ts:503-511` 生产代码无引用（对话框用的是 `providers.ts` 里每个定义的 `credentialHint` 字段），只剩 `domain.test.ts` 在测它，且文案已与 providers.ts 漂移（还写着「Windows DPAPI」总述）。建议删除函数与对应测试。

### 21. index.html 静态初值过时

`index.html:50` coverage 硬编码 `0 / 9`（实际目录 17 家）、`:15` 与 `:162` 版本号 `0.1.5`（实际 0.1.6）。启动后都会被 JS 覆盖，仅首帧/极端情况可见，顺手更新即可。

### 22. 杂项

- `src/app.ts:1274-1276`：`close-confirm-dialog` 分支缺 `return`（当前后续条件都不匹配，无行为影响，仅一致性问题；末尾 `close-rename-dialog` 同）。
- `src/app.ts:1153-1164`：`applyAutoSync(NaN)` 时 `NaN <= 0` 为 false，`setInterval(…, NaN)` 按 0ms 处理会疯狂连打同步。当前 `data-seconds` 来自静态 HTML 无风险，建议加 `Number.isFinite` 防御。
- `src-tauri/src/app.rs:641-651`：导入时 `metadata` 大小检查与 `read` 之间存在 TOCTOU（本地桌面应用、用户自选文件，风险很低，仅记录）。

---

## 做得好的地方（保持）

- **凭据安全**：DPAPI/Keychain 存储、`set_sensitive` 头、`zeroize`、明文只在导出文件落地一次、导入前离线格式校验、导出/导入的大小与数量上限、备注清洗前后端镜像。
- **不可信输入防御**：路径穿越三重防护（`is_safe_id`、`validate_qwen_endpoint` 域名白名单、`valid_xai_team_id`/`valid_google_project_id`）；JSON 解析处处 `checked_add`、范围与 `is_finite` 校验、`MAX_DETAIL_ENTRIES` 上限、`analytics_label` 清洗控制字符；快照序列化测试明确断言不泄漏 email/用户名。
- **并发**：daily history 写锁 + 压力测试；Windows 单实例启动竞态的 mutex 补丁有针对性测试。
- **前端**：全部 DOM 写入走 `textContent`，无 `innerHTML`，注入面干净；图表无第三方运行时；测试覆盖核心选择器逻辑（76 用例）。

## 验证记录

- `npm test`：4 个文件 76 个用例全部通过（2026-09-24）。
- `cargo check`（Windows host）：通过（2026-09-24）。`cargo test` 未运行（耗时考虑）；#3 的 Linux 结论为静态分析。

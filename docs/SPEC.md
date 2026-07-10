# Spec: LLM Usage 桌面用量仪表盘

## Objective

构建一个 Windows 优先、体积极小、界面精致的桌面应用，用于实时查看国内主流 LLM 的调用量、Token 用量、请求状态与限流冷却时间。

核心用户是同时使用多家模型 API 的个人开发者。应用通过仅监听本机的 OpenAI 兼容代理记录真实请求，避免依赖各供应商并不统一、甚至不存在的“今日用量”接口。供应商若提供余额或账单接口，则作为增强数据源显示官方数据。

## Assumptions to Validate

1. 第一目标平台是 Windows 10/11；架构保留 macOS/Linux 可移植性，但首版不以它们作为发布门槛。
2. “今天用了多少”的权威口径是经过本应用本地代理的请求；无法追溯安装前或绕过代理的调用。
3. API Key 由用户自行提供，保存在操作系统凭据库，不写入 SQLite、日志或前端存储。
4. 首发内置 GLM（智谱）、Kimi（月之暗面）、MiniMax；其他 OpenAI 兼容服务可用自定义供应商配置接入。
5. 冷却时间优先读取 `Retry-After` 和供应商限流响应头；没有可靠响应头时显示“未知”，不伪造倒计时。
6. 首版提供本地代理和仪表盘，不拦截或修改其他应用的网络流量；用户显式把客户端 Base URL 指向本应用。

## Product Scope

### Dashboard

- 展示今日请求数、输入 Token、输出 Token、总 Token、成功率。
- 按供应商展示用量、最近调用时间、连接状态和冷却倒计时。
- 最近请求流实时更新，包含供应商、模型、Token、耗时、状态；不保存提示词或回复正文。
- 支持手动刷新和自动刷新，日期边界使用本机时区。

### Providers

- 内置：GLM、Kimi、MiniMax。
- 扩展内置候选：DeepSeek、百炼/Qwen、腾讯混元、百度千帆、豆包、零一万物（仅在端点与鉴权可稳定验证后启用）。
- 自定义 OpenAI 兼容供应商：名称、Base URL、API Key、可选模型过滤。
- 每个供应商可单独启用、停用与测试连接。

### Local Proxy

- 默认监听 `127.0.0.1` 的随机可用端口，不对局域网开放。
- 提供 OpenAI 兼容的聊天补全入口；流式与非流式请求均透传。
- 从 JSON 或 SSE 最终块解析 `usage`；缺少 usage 时记录请求但明确标记 Token 未知。
- 读取限流响应头和 HTTP 429，形成可解释的冷却状态。
- 不记录 Authorization、请求正文、响应正文或用户提示词。

### Data and Privacy

- SQLite 只保存供应商非敏感配置、聚合所需请求元数据和应用设置。
- API Key 存入 Windows Credential Manager（跨平台时映射到系统 keyring）。
- 支持清除历史记录与删除供应商凭据。
- 所有供应商 URL 必须为 HTTPS；仅本地回环代理允许 HTTP。

### Out of Scope for v1

- 抓取网页控制台、浏览器 Cookie 或逆向私有账单接口。
- 云同步、账号系统、团队共享。
- 根据输入文本估算供应商账单金额（价格变化快且不同套餐口径不一致）。
- 全局 MITM、系统代理劫持或透明抓包。

## Tech Stack

- Tauri 2：系统 WebView 桌面壳，减少安装体积。
- Rust：代理、SQLite、凭据库、事件推送和供应商适配。
- 原生 TypeScript + HTML + CSS：不引入 React/Vue 运行时，控制体积与启动速度。
- Vite：前端开发和静态构建。
- SQLite：单文件本地数据库，迁移由 Rust 管理。

选择原生前端而非大型 UI 框架，是为了以更少依赖获得更小包体；设计系统使用 CSS 自定义属性、语义化组件类和原生控件。

## Internal Contracts

```ts
type ProviderKind = "glm" | "kimi" | "minimax" | "openai_compatible";

interface ProviderSummary {
  id: string;
  name: string;
  kind: ProviderKind;
  isEnabled: boolean;
  status: "READY" | "COOLDOWN" | "UNCONFIGURED" | "ERROR";
  cooldownEndsAt: string | null;
  today: {
    requests: number;
    inputTokens: number;
    outputTokens: number;
    unknownTokenRequests: number;
  };
}
```

Tauri commands use typed request/response objects and a single error shape: `{ code, message }`. External responses are validated at the Rust boundary. New fields remain optional or additive.

## Commands

- Install: `npm install`
- Develop: `npm run tauri dev`
- Frontend test: `npm test`
- Rust test: `cargo test --manifest-path src-tauri/Cargo.toml`
- Type check: `npm run typecheck`
- Build: `npm run tauri build`

## Project Structure

- `src/` — 原生 TypeScript UI、样式与前端测试。
- `src-tauri/src/` — Rust 应用、代理、存储、凭据及供应商模块。
- `src-tauri/migrations/` — SQLite 迁移。
- `docs/` — 产品规格、架构决策和发布说明。
- `tests/` — 跨模块集成测试（需要时）。

## Code Style

```rust
pub fn cooldown_deadline(now: DateTime<Utc>, retry_after: &str) -> Option<DateTime<Utc>> {
    let seconds = retry_after.trim().parse::<i64>().ok()?;
    now.checked_add_signed(chrono::Duration::seconds(seconds))
}
```

- Rust 使用 `rustfmt` 与 `clippy -D warnings`；不使用 `unwrap()` 处理外部输入。
- TypeScript 开启 strict；DOM 更新使用 `textContent`，不把外部数据写入 `innerHTML`。
- 时间在存储层使用 UTC ISO 8601，在 UI 按本机时区显示。

## Testing Strategy

- Rust 单元测试：usage 解析、SSE 解析、限流响应头、日期聚合、URL 校验。
- Rust 集成测试：本地 mock 上游验证流式/非流式透传、鉴权不落日志、429 冷却。
- 前端单元测试：格式化、倒计时、状态映射和视图模型。
- 浏览器运行验证：仪表盘在 320/768/1024/1440 宽度无溢出，键盘可访问，无控制台错误。
- 桌面冒烟测试：首次启动、添加供应商、代理调用、实时刷新、重启后统计保留。

## Boundaries

### Always

- 外部输入在边界校验；仅绑定回环地址；API Key 使用系统凭据库。
- 行为修改先写失败测试，完成一个纵向切片后运行测试和构建。
- UI 提供加载、空、错误、未配置和冷却状态。

### Ask First

- 改为监听局域网、增加自动读取其他程序凭据、增加遥测或云同步。
- 引入抓包证书、浏览器自动化或供应商非公开接口。

### Never

- 提交、记录或显示完整 API Key。
- 保存提示词、模型回复正文或 Authorization 头。
- 在缺乏可靠数据时伪造 Token、余额或冷却时间。

## Success Criteria

1. Windows 上可安装并启动，发布包目标小于 15 MB（不含系统 WebView2 运行时）。
2. GLM、Kimi、MiniMax 均可配置并通过本地代理完成一次真实或 mock 合约调用。
3. 非流式和 SSE 流式调用都能记录请求；存在 usage 时 Token 统计准确，不存在时明确显示未知。
4. 仪表盘在请求完成后 1 秒内更新，无需整页刷新。
5. 收到 HTTP 429 或标准限流头后显示冷却截止时间与实时倒计时；未知时不猜测。
6. 重启应用后当日统计保留，跨本地自然日正确归零并可查询历史日期。
7. API Key 不出现在数据库、应用日志、前端存储和 Git 历史中。
8. 自动化测试、TypeScript 严格检查、Rust clippy 和生产构建全部通过。
9. UI 支持键盘操作，正常文本对比度达到 WCAG AA，并覆盖加载/空/错误状态。

## Open Questions

1. 是否确认 Windows 优先，以及安装包小于 15 MB 的目标？
2. 是否接受“只有经过本地代理的请求才能精确统计”这一事实边界？
3. 首版是否需要价格/人民币成本估算，还是只做请求与 Token 用量？
4. 是否有必须首发支持、优先级高于自定义 OpenAI 兼容配置的国内供应商？

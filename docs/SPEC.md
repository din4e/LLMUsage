# Spec: LLM Usage 桌面用量仪表盘

## Objective

构建一个 Windows 优先、体积极小、界面精致的桌面应用，用于在线查看国内外主流 LLM 账户的今日调用量、Token 用量、人民币成本、套餐余额与限流冷却时间。

核心用户是同时使用多家模型 API 或 Coding Plan 的个人开发者。应用以供应商官方在线用量、套餐余量和余额接口为第一数据源；本地 OpenAI 兼容代理只用于补足官方接口没有提供的请求级明细，不是使用应用的前提。

## Confirmed Product Decisions

1. 第一目标平台是 Windows 10/11，发布包目标小于 15 MB（不含系统 WebView2）；架构保留 macOS/Linux 可移植性。
2. “今天用了多少”优先采用供应商官方在线统计，可包含其他客户端产生的用量；若无公开查询接口，则降级显示官方余额或本地可观察调用，并清楚标注口径。
3. API Key 由用户自行提供，使用 Windows DPAPI 绑定当前用户加密后保存在应用数据目录，不写入缓存、日志或前端存储。
4. 首发内置 GLM、Kimi、MiniMax，并尽可能覆盖 DeepSeek、Qwen/百炼、豆包/火山方舟、腾讯混元、百度千帆、硅基流动，以及 OpenAI、Anthropic、Gemini、OpenRouter、Mistral、Groq、Together AI、xAI；国内站和国际站分别建模。
5. 冷却时间优先读取官方/兼容用量接口的重置时间，其次读取 `Retry-After` 和供应商限流响应头；没有可靠数据时显示“未知”。
6. 首版提供在线同步与仪表盘；本地代理是可选增强，不拦截或修改其他应用的网络流量。
7. 同时显示供应商原币成本和按汇率折算的人民币估算；估算值与官方账单值明确区分。
8. 可采用社区验证的、使用 API Key 鉴权但未被供应商正式公开承诺的端点；必须标为实验性、严格校验并可独立降级，禁止使用网页登录 Cookie。
9. 外币成本使用每日公开汇率直接换算人民币，并允许界面切换原币/人民币；离线使用上次缓存汇率。

## Product Scope

### Dashboard

- 展示今日请求数、输入 Token、输出 Token、总 Token、人民币成本和数据更新时间。
- 按供应商展示在线用量、套餐余量、余额、数据口径、连接状态和冷却倒计时。
- 最近请求流实时更新，包含供应商、模型、Token、耗时、状态；不保存提示词或回复正文。
- 支持手动刷新和可配置自动刷新，自动刷新间隔是非敏感本地 UI 设置；日期边界使用本机时区，避免高频轮询消耗配额。
- 每个指标标注来源：`官方用量`、`官方余额`、`本地观测`、`估算`或`不可用`。
- 仪表盘只渲染已经保存凭据的供应商；未配置供应商集中放在“添加供应商”目录中，不占用主列表空间。
- 同一供应商支持配置多个实例（例如多个 GLM 或 Kimi 账号 Key）；每个实例在主列表各自成行，实例 2 起显示实例徽标，凭据、快照缓存与每日汇总按实例隔离，Token 用量可持续分别查看。实例可编辑备注名称，显示为主列表徽标与状态提示；备注属于本地 UI 设置，删除实例时一并清除。
- 每个供应商的完整明细使用原生可访问折叠控件，默认收起；摘要指标始终可见。
- 信息密度通过更小的留白和默认收起明细提高，但正文和辅助文字不得小于 12px。

### Providers

- 核心国内：GLM、Kimi、MiniMax、DeepSeek、Qwen/百炼、豆包/火山方舟、腾讯混元、百度千帆、硅基流动。
- 国际平台：OpenAI、Anthropic、Gemini、OpenRouter、Mistral、Groq、Together AI、xAI。
- 同品牌中国区与国际区分别配置；仅经官方文档或合约测试验证的能力才标记可用。
- 自定义 OpenAI 兼容供应商：名称、Base URL、API Key、可选模型过滤。
- 每个供应商可单独启用、停用与测试连接。
- 同一供应商可添加多个实例：实例 id 为 `供应商 ID`（实例 1）或 `供应商 ID_序号`（实例 2 起，如 `kimi_cn_2`），在 Rust 边界校验字符集与长度；凭据库、快照与每日汇总均按实例 id 存取，供应商目录中已配置的供应商可继续添加实例。
- 已保存的凭据可在「修改配置」对话框中查看：打开时自动回填原值，密码字段默认掩码显示并可切换明文；关闭对话框即从界面清除。

### Online Usage and Cost

- 数据源优先级：官方套餐/用量 API > 官方余额 API > 调用响应 `usage` 与限流头 > 本地聚合；禁止网页 Cookie 抓取和私有接口逆向。
- 适配器声明能力：今日用量、套餐余量、余额、冷却、调用明细、模型价格；UI 只展示真实具备的能力。
- 套餐接口返回多个窗口或模型时，摘要下方默认展开全部已验证明细，不得只保留第一项；身份对象和未知原始字段不进入前端或缓存。
- MiniMax `model_remains` 同时支持计数型额度和仅返回剩余百分比的额度。Coding Plan 新结构中的 `current_*_usage_count` 按“剩余次数”解释；不得因 `total_count = 0` 而丢弃带 `remaining_percent` 的 general/text 额度。
- 定价表包含来源 URL、生效日期、币种及输入/输出/缓存 Token 单价，可独立更新。
- 人民币换算保存汇率来源和更新时间；离线时使用上次汇率并标记“估算”。官方扣费优先于估算。
- 余额变化不能可靠推出今日成本时不做差值猜测。

### Optional Local Proxy

- 默认监听 `127.0.0.1` 的随机可用端口，不对局域网开放。
- 提供 OpenAI 兼容的聊天补全入口；流式与非流式请求均透传。
- 从 JSON 或 SSE 最终块解析 `usage`；缺少 usage 时记录请求但明确标记 Token 未知。
- 读取限流响应头和 HTTP 429，形成可解释的冷却状态。
- 不记录 Authorization、请求正文、响应正文或用户提示词。

### Data and Privacy

- 使用非敏感 JSON snapshot cache 保存上次在线同步结果，并按本地日期保存供应商日汇总用于趋势查询；请求级历史或本地代理明细再迁移到 SQLite。
- API Key 在 Windows 使用当前用户级 DPAPI 加密；其他平台发布前映射到各自系统 keyring。
- 支持清除历史记录与删除供应商凭据。
- 所有供应商 URL 必须为 HTTPS；仅本地回环代理允许 HTTP。

### Out of Scope for v1

- 抓取网页控制台、浏览器 Cookie 或逆向私有账单接口。
- 云同步、账号系统、团队共享。
- 从网页控制台抓取数据或要求用户粘贴登录 Cookie。
- 全局 MITM、系统代理劫持或透明抓包。

## Tech Stack

- Tauri 2：系统 WebView 桌面壳，减少安装体积。
- Rust：凭据库、snapshot cache、事件推送和供应商适配；可选代理在后续切片加入。
- 原生 TypeScript + HTML + CSS：不引入 React/Vue 运行时，控制体积与启动速度。
- Vite：前端开发和静态构建。
- JSON cache：缓存非敏感的最新同步结果与每日供应商汇总，保持安装包体积；SQLite 留作请求级历史明细和本地代理阶段。

选择原生前端而非大型 UI 框架，是为了以更少依赖获得更小包体；设计系统使用 CSS 自定义属性、语义化组件类和原生控件。

## Internal Contracts

```ts
type ProviderKind =
  | "glm" | "kimi" | "minimax" | "deepseek" | "qwen" | "doubao"
  | "hunyuan" | "qianfan" | "siliconflow" | "openai" | "anthropic"
  | "gemini" | "openrouter" | "mistral" | "groq" | "together" | "xai"
  | "openai_compatible";

interface ProviderSummary {
  id: string;
  name: string;
  kind: ProviderKind;
  isEnabled: boolean;
  status: "READY" | "COOLDOWN" | "UNCONFIGURED" | "ERROR";
  cooldownEndsAt: string | null;
  source: "OFFICIAL_USAGE" | "OFFICIAL_BALANCE" | "LOCAL_OBSERVED" | "ESTIMATED" | "UNAVAILABLE";
  balance: { amount: number; currency: string } | null;
  estimatedCostCny: number | null;
  today: {
    requests: number;
    inputTokens: number;
    outputTokens: number;
    unknownTokenRequests: number;
  };
}
```

Tauri commands use typed request/response objects and a single error shape: `{ code, message }`. External responses are validated at the Rust boundary. New fields remain optional or additive.

复杂凭据仍作为一个 DPAPI 加密文档保存，但在配置界面分字段输入，并在 Rust 边界反序列化和校验：

- OpenAI / Codex API：OpenAI Organization Admin API Key；统计 API 组织的 completions 用量和成本，不冒充 ChatGPT 个人订阅剩余额度。
- Claude Code：Anthropic Admin API Key；读取官方 Claude Code Analytics 日汇总，不自动读取 Claude Code 本地 OAuth 凭据。
- Gemini Code Assist：Google Cloud Project ID + 用户主动提供的 OAuth Access Token；读取 Cloud Monitoring 指标，令牌过期后要求用户重新配置，不读取 `gcloud` 或浏览器凭据。
- Qwen / 百炼：中国区和国际区分别配置 Prometheus HTTP API 地址 + 最小权限 AccessKey ID/Secret；读取官方 `model_usage` 和 `model_call_count`，不使用禁止自动化访问的 Coding Plan Key。

## Commands

- Install: `npm install`
- Develop: `npm run tauri dev`
- Frontend test: `npm test`
- Rust test: `cargo test --manifest-path src-tauri/Cargo.toml`
- Type check: `npm run typecheck`
- Build: `npm run tauri build`

## Project Structure

- `src/` — 原生 TypeScript UI、样式与前端测试。
- `src-tauri/src/` — Rust 应用、在线同步、可选代理、存储、凭据及供应商模块。
- `src-tauri/src/cache.rs` — 非敏感 snapshot cache。
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

- Rust 单元测试：在线响应解析、能力映射、usage/SSE、限流响应头、日期聚合、分币种成本和 URL 校验。
- Rust 集成测试：每个供应商使用本地 mock 验证官方查询合约；可选代理验证流式/非流式透传、鉴权不落日志和 429 冷却。
- 前端单元测试：格式化、倒计时、状态映射和视图模型。
- 浏览器运行验证：仪表盘在 320/768/1024/1440 宽度无溢出，键盘可访问，无控制台错误。
- 桌面冒烟测试：首次启动、添加供应商、在线同步、成本显示、冷却倒计时、重启后统计保留、开机自启动开关与系统启动项注册。

## Boundaries

### Always

- 外部输入在边界校验；仅绑定回环地址；API Key 使用系统凭据库。
- 行为修改先写失败测试，完成一个纵向切片后运行测试和构建。
- UI 提供加载、空、错误、未配置和冷却状态。

### Ask First

- 改为监听局域网、增加自动读取其他程序凭据、使用网页登录态、增加遥测或云同步。
- 引入抓包证书、浏览器自动化或供应商非公开接口。

### Never

- 提交、记录或显示完整 API Key。
- 保存提示词、模型回复正文或 Authorization 头。
- 在缺乏可靠数据时伪造 Token、余额或冷却时间。

## Success Criteria

1. Windows 上可安装并启动，发布包目标小于 15 MB（不含系统 WebView2 运行时）。
2. GLM、Kimi、MiniMax 均可配置并完成一次官方在线数据源的真实或 mock 合约同步；无公开接口的能力明确降级。
3. 核心国内与国际供应商均有能力声明、区域端点和测试覆盖；自定义 OpenAI 兼容配置可用。
4. 在线同步完成后仪表盘在 1 秒内更新，无需整页刷新，并展示来源与最后更新时间。
5. 官方套餐返回重置时间，或调用收到 HTTP 429/标准限流头时，显示冷却截止时间与实时倒计时；未知时不猜测。
6. 官方或本地用量存在时正确显示当日统计；缓存跨重启保留最近一次同步结果与按供应商划分的每日汇总，并支持最近 7 日、最近 30 日和全部趋势查询。
7. Kimi 展示周额度、全部限额窗口、并发/总额度；MiniMax 展示全部模型的当前/周窗口及已用、剩余、上限和时间字段。
8. MiniMax 的 general/text、video、image、speech、music 等返回项全部进入折叠明细；百分比型和次数型额度均有回归测试。
9. 未配置供应商不出现在主列表；可从供应商目录配置。所有完整明细默认折叠、可键盘展开，紧凑布局在 320px 宽度不产生横向滚动。
10. OpenAI/Codex API、Claude Code Analytics、Gemini Code Assist Monitoring、Qwen/百炼 Monitoring 只在满足其官方凭据要求时可配置，并明确展示统计口径与限制。
11. API Key 不出现在数据库、应用日志、前端存储和 Git 历史中。
12. 自动化测试、TypeScript 严格检查、Rust clippy 和生产构建全部通过。
13. UI 支持键盘操作，正常文本对比度达到 WCAG AA，并覆盖加载、空、错误和能力不可用状态。
14. 成本同时显示官方原币值（若有）与人民币值；估算可追溯到模型价格、生效日期和汇率时间。

## Open Questions

None for the first implementation slice. New credential types or browser-session access still require explicit approval.

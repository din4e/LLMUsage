# Provider Capability Matrix

Last verified: 2026-08-16

This matrix is the implementation contract for online provider adapters. A provider is only marked as supporting a capability when a public, official API documents it. Console-only data is not treated as an API, and private browser endpoints or login cookies are out of scope.

## Capability Levels

- **Official API** — can be queried by the desktop app using documented credentials.
- **Response-derived** — observed from documented inference responses or rate-limit headers; requires calls to pass through the optional local observer.
- **Console only** — the provider documents the data in its web console but no public query API has been verified.
- **Unverified** — no supported public mechanism has been verified yet.

## Verified Providers

| Provider / region | Online balance | Online usage | Plan reset / cooldown | Cost | Adapter decision |
|---|---|---|---|---|---|
| MiniMax China, pay-as-you-go | Unverified | Response-derived | Response-derived | Estimate from usage and dated price table | Enable request observation; do not claim account-wide usage |
| MiniMax China, Token Plan | Experimental API-key endpoint: remaining quota | Experimental endpoint: plan remaining usage | Reset fields when present; documented resource-limit error otherwise | Subscription usage, not per-call RMB cost | Experimental online plan adapter |
| Kimi/Moonshot China API | Official API balance | Console only; daily billing may update the next morning | Response-derived for inference rate limits | Console only or response-derived estimate | Balance adapter plus optional observer |
| Kimi Code | Not applicable | Experimental API-key endpoint returns normalized quota | Experimental endpoint returns 5-hour and weekly reset timestamps | Included in subscription | Experimental online adapter with Moonshot balance fallback |
| GLM China API / Coding Plan | Community-verified API-key endpoints | Community-verified model and tool usage endpoints | Community-verified quota endpoint returns reset timestamps; official FAQ documents 5-hour and weekly limits | Included in subscription or response-derived estimate | Experimental online adapter with strict validation and graceful fallback |
| DeepSeek China API | Official API | Console only / response-derived | Response-derived | Response-derived estimate | Balance adapter plus optional observer |
| SiliconFlow China API | Official API: user info balance | Console only / response-derived | Response-derived | Response-derived estimate | Balance adapter plus optional observer |
| SiliconFlow Global API | Official API: user info balance | Console only / response-derived | Response-derived | Response-derived estimate | Balance adapter plus optional observer |
| OpenRouter Global | Official API: credits | Response-derived / dashboard analytics | Response-derived | Official credits remaining; request-level estimate from usage | Credits adapter plus optional observer |
| Volcengine Ark / Doubao China | Official billing API | Official `GetInferenceUsage` API | API/response-derived where available | Official billing API | First-class online usage and billing adapter |
| OpenAI / Codex API | Not applicable | Official Organization Usage API with Admin key | API tier limits are separate; personal ChatGPT plan remaining quota is not exposed | Official Organization Costs API in USD | Admin-key adapter; label as API organization data, not ChatGPT subscription quota |
| Claude Code | Not applicable | Official Claude Code Analytics API with Admin key | Subscription remaining quota is not returned | Estimated cost by model in USD cents | Daily UTC analytics adapter; aggregate without exposing actor email |
| Anthropic Messages API | Not applicable | Official `usage_report/messages` API with Admin key | Not returned | Not returned (token counts only) | Daily per-model token adapter (input / cache read / cache write / output); no request count field |
| xAI / Grok Global | Official Management API: `prepaid/balance` (management key + team id) | Balance change history (`changes[]`) attributes daily spend when timestamps parse | Not returned | Balance and spend in USD cents | Management-key balance adapter; inference keys cannot query billing |
| PPIO 派欧云 China | Official Management API: billing balance detail (Bearer API key) | Console only | Not returned | Not returned | Balance adapter; amounts arrive as strings in 1/10,000 CNY |
| Mistral Global | Console only (billing page); no API-key endpoint | Console only | Not returned | Console only | Do not implement; community tools rely on browser cookies, which are out of scope |
| Groq Global | Not applicable (rate-limit based) | Response-derived rate-limit headers; usage dashboard is console only | Response-derived | Console only | Do not implement balance; optional observer only |
| Together AI Global | Console only (credits page) | Console only | Not returned | Console only | Do not implement; no documented programmatic credits endpoint |
| Cerebras Global | Console only (Overview tab) | Metrics API is limited to dedicated-endpoint customers | Response-derived | Console only | Do not implement for standard API keys |
| Z.AI / GLM Coding Plan Global | Console only | Community discussions only; no verified API-key usage endpoint | Console FAQ documents 5-hour refresh | Included in subscription | Not added; the China community endpoint is not verified for z.ai keys |
| Gemini Code Assist | Not applicable | Official Cloud Monitoring metrics for API calls and used tokens | Published fixed quota; remaining personal quota is not returned | Not returned by monitoring metrics | Project + explicit OAuth access-token adapter; never read local Google credentials |
| Alibaba Model Studio / Qwen China & Global | Not applicable | Official private Prometheus metrics `model_usage` and `model_call_count` | Coding Plan remaining quota remains console-only | Billing is separate from monitoring | Prometheus URL + least-privilege AccessKey adapter; Coding Plan keys are rejected |

## Official Endpoint Contracts Verified

### MiniMax China Token Plan

- Primary remaining-plan endpoint: `GET https://www.minimaxi.com/v1/token_plan/remains`
- Same-region fallback: `GET https://api.minimaxi.com/v1/token_plan/remains`
- Authentication uses `Authorization: Bearer <MINIMAX_API_KEY>`.
- The response may expose count limits, remaining percentages, or both. The current official CLI fixtures include `general` with zero count limits plus `current_*_remaining_percent`, and `video` with explicit remaining counts.
- In `model_remains`, `current_interval_usage_count` and `current_weekly_usage_count` are remaining counts despite their names. The app derives used counts as `total - remaining` and never fabricates counts from a percentage-only entry.
- Every validated `model_remains` item is retained. Current and weekly windows are rendered separately with model/resource name, used/remaining/limit or remaining percent, status, boost, start/end timestamps and remaining duration when present.
- China and international Token Plan keys are separate products and must not share endpoint defaults.

### Kimi Code

- Experimental usage endpoint: `GET https://api.kimi.com/coding/v1/usages`
- Authentication uses the Kimi Code membership API Key, commonly prefixed `sk-kimi-`; it is not interchangeable with a Moonshot Open Platform key.
- The observed response exposes the weekly quota in `usage` and the rolling 5-hour window in `limits`, with RFC 3339 reset timestamps.
- Every validated entry in `limits` is retained rather than collapsing the response to one progress bar. `parallel` and `totalQuota` are shown when returned.
- The `user` object and unknown raw fields are intentionally excluded from the snapshot/cache; only normalized quota data crosses the backend/frontend boundary.
- The China adapter recognizes the `sk-kimi-` key family and contacts only the Kimi Code endpoint. Other Kimi China keys use the official Moonshot balance endpoint, preventing a credential from being sent across the two product surfaces.

### Kimi/Moonshot China API

- Balance endpoint: `GET https://api.moonshot.cn/v1/users/me/balance`
- Authentication: `Authorization: Bearer <MOONSHOT_API_KEY>`.
- Response exposes available, voucher, and cash balances.
- This is the Kimi/Moonshot API account balance, not Kimi Code subscription quota or cooldown.
- Official help describes daily per-model usage and cost in the console, but says daily billing is updated by 07:00 the following day. This is not a real-time public usage API.

### DeepSeek China API

- Balance endpoint: `GET https://api.deepseek.com/user/balance`
- Returns total available balance and balance components.
- The inference API is OpenAI compatible and returns usage for observed calls; account-wide daily usage remains a separate console capability unless a public endpoint is verified.

### SiliconFlow API

- China user-info endpoint: `GET https://api.siliconflow.cn/v1/user/info`
- Global user-info endpoint: `GET https://api.siliconflow.com/v1/user/info`
- Authentication: `Authorization: Bearer <SILICONFLOW_API_KEY>`.
- Response exposes `balance`, `chargeBalance`, `totalBalance`, and account status.

### OpenRouter API

- Credits endpoint: `GET https://openrouter.ai/api/v1/credits`
- Authentication: `Authorization: Bearer <OPENROUTER_MANAGEMENT_KEY>`.
- Response exposes total purchased credits and total usage. Remaining USD credits are calculated as `total_credits - total_usage`.

### Volcengine Ark / Doubao China

- `GetInferenceUsage` is a documented control-plane API for inference usage.
- The documented usage view includes request tokens, input tokens, and output tokens with hourly or daily granularity.
- Volcengine Billing Center exposes public APIs including account balance, bill overview, bill details, and daily amortized cost.
- This adapter needs Volcengine access-key signing rather than a simple model API key.

### GLM China experimental monitor contract

The MIT-licensed `LaughSmiles/glm-key-monitor` project demonstrates three API-key-authenticated endpoints on `https://open.bigmodel.cn`:

- `GET /api/monitor/usage/quota/limit` — quota limits and reset timestamps.
- `GET /api/monitor/usage/model-usage` — model call counts and token usage for a time range.
- `GET /api/monitor/usage/tool-usage` — tool usage for a time range.

Requests send the BigModel API key directly in the `Authorization` header and accept optional `startTime` and `endTime` query parameters. The observed quota schema includes a plan `level` and a `limits` array whose entries contain `type`, `unit`, `number`, `percentage`, `nextResetTime`, optional current usage, and optional per-model usage details. Legacy responses use `TOKENS_LIMIT`; GLM Max responses verified on 2026-08-24 use `CREDIT_LIMIT` for both a 5-hour window (`unit=3`, `number=5`) and a weekly window (`unit=6`, `number=1`). The adapter prefers `CREDIT_LIMIT` when both generations coexist, falls back to `TOKENS_LIMIT`, preserves every recognized window in details, and uses the shortest window for the dashboard summary and cooldown.

Verified 2026-08-19: a valid pay-as-you-go key without a Coding Plan subscription gets HTTP 200 with `{"code":500,"msg":"当前用户不存在coding plan","success":false}` from both endpoints. The adapter maps this body to the dedicated `GLM_NO_CODING_PLAN` error and refuses to save the credential; the key itself is not invalid.

Negative verification 2026-08-19 — no pay-as-you-go data source exists: Zhipu publishes no balance/usage/account API (the docs.bigmodel.cn sitemap enumerates only model, tool, batch, file, knowledge-base, and agent APIs); probed and rejected with 404: `/api/paas/v4/users/balance`, `/api/paas/v4/balance`, `/api/paas/v4/users/me`, `/api/paas/v4/dashboard/billing/{subscription,usage,credit_grants}`; community tooling (cc-switch #1588, glm-key-monitor) queries only the Coding-Plan monitor endpoints above; the official fee FAQ points users to the console finance page. A non-Coding-Plan GLM adapter is therefore blocked upstream, not by this app.

These endpoints are not currently documented in GLM's public official API reference. They therefore remain an **experimental compatibility source**, not an official API capability. Implementation requirements:

1. Validate the full response before persisting or displaying any value.
2. Never log the request headers or API key.
3. Apply a conservative refresh interval and exponential backoff.
4. On 401/403, delete no credentials and show an actionable authentication error.
5. On 404/schema drift, disable only online monitoring and retain response-derived/local data.
6. Identify the source in the UI as “兼容接口（非官方承诺）”.

### Anthropic Messages Usage Report

- Endpoint: `GET https://api.anthropic.com/v1/organizations/usage_report/messages`.
- Authentication: Admin API key via `x-api-key` plus `anthropic-version: 2023-06-01`.
- Query parameters used: `starting_at`, `ending_at` (RFC 3339), `bucket_width=1d`, `group_by[]=model`, `limit=31`.
- Token fields per result row: `uncached_input_tokens`, `cache_read_input_tokens`, `cache_creation.ephemeral_5m_input_tokens` + `ephemeral_1h_input_tokens`, and `output_tokens`.
- The report exposes no request count and no cost; the adapter therefore reports token totals only and leaves requests and cost empty.
- Buckets are UTC day boundaries; empty buckets are included with empty `results`.

### xAI Management Billing

- Endpoint: `GET https://management-api.x.ai/v1/billing/teams/{team_id}/prepaid/balance`.
- Authentication: management key (console → Settings → Management Keys) as a Bearer token; inference keys (`xai-…`) are rejected.
- Money is reported as USD cents in string form inside `amount.val` / `total.val`.
- `changes[]` entries carry `changeOrigin` (`PURCHASE`, `REFUND`, and `AUTO_PURCHASE` negative; `SPEND` positive) and `createTs` timestamps. Daily spend is the sum of in-range `SPEND` entries with parseable timestamps; unparseable stamps are skipped rather than guessed.
- Team ids are restricted to alphanumeric and hyphen before being placed in the URL path.

### PPIO Billing Balance Detail

- Endpoint: `GET https://api.ppio.com/openapi/v1/billing/balance/detail`.
- Authentication: standard Bearer API key from the PPIO open platform.
- Response fields `availableBalance`, `cashBalance`, and `creditLimit` arrive as strings in units of 1/10,000 CNY and are converted to yuan at the Rust boundary.
- Usage and per-key breakdowns remain console-only per the official FAQ.

## Plan Semantics Verified

### GLM Coding Plan

- Uses both a 5-hour limit and a weekly limit.
- Exhausted quota waits for the next window and does not fall through to normal account resources.
- Coding Plan endpoints differ from standard API endpoints.
- Public documentation points users to the web usage page; no public quota-query API has yet been verified.

### Kimi Code

- Uses a rolling 5-hour frequency window and a quota that refreshes every 7 days from the subscription start date.
- All logged-in devices and plan keys share the quota.
- The public product documentation points users to the console; the API-key usage endpoint is therefore treated as experimental despite being confirmed in the provider's community support forum.

## Security and UX Rules

1. Never accept session cookies, browser storage exports, or copied authorization requests.
2. Never label console-only or response-derived figures as official online usage.
3. Show the data source, observed interval, provider update delay, and last successful synchronization beside every metric.
4. Request cloud IAM permissions only for the exact usage/billing read actions needed by an adapter.
5. Store model keys and cloud access secrets only in the operating-system credential vault.
6. Region selection changes endpoints, currency, pricing, and credential namespace together.

## Newly Verified Official Contracts

- OpenAI Organization Usage: `GET /v1/organization/usage/completions`; Organization Costs: `GET /v1/organization/costs`; both require an OpenAI Admin API Key.
- Claude Code Analytics: `GET https://api.anthropic.com/v1/organizations/usage_report/claude_code?starting_at=YYYY-MM-DD`; requires `x-api-key` with an Anthropic Admin API Key and reports daily UTC metrics.
- Gemini Code Assist metrics are read through Cloud Monitoring from `code_assist/api_calls_count` and `code_assist/used_tokens_count`; private project metrics require OAuth and Monitoring Viewer permission.
- Alibaba Model Studio advanced monitoring exposes Prometheus `model_usage` and `model_call_count` through the workspace's private HTTP API with Basic authentication using an Alibaba Cloud AccessKey pair.

## Research Queue

- GLM standard API balance: verified absent as of 2026-08-19 (see negative verification above); re-check if Zhipu announces a billing API. Monitor-endpoint official publication/stability still open.
- Official publication/stability of the Kimi Code usage endpoint.
- MiniMax international Token Plan endpoint and response schema.
- Alibaba Cloud Model Studio Coding Plan/Token Plan remaining quota remains console-only and is not queried with plan keys because official terms prohibit custom automated clients.
- Tencent Hunyuan, Baidu Qianfan, SiliconFlow account-wide usage and billing APIs.
- Mistral, Groq, Together AI, xAI, Tencent Hunyuan and Baidu Qianfan organization usage/cost endpoints and required admin credentials.

## Official Sources

- MiniMax Token Plan FAQ: https://platform.minimaxi.com/docs/token-plan/faq
- MiniMax API overview: https://platform.minimaxi.com/docs/api-reference/api-overview
- Kimi balance API: https://platform.kimi.com/docs/api/balance
- Kimi balance and usage help: https://www.kimi.com/help/kimi-api/api-balance-and-usage
- Kimi Code benefits: https://www.kimi.com/zh-cn/help/kimi-code/benefits
- Kimi Code experimental usage endpoint confirmation: https://forum.moonshot.ai/t/error-code-429-were-receiving-too-many-requests-at-the-moment/191
- GLM Coding Plan FAQ: https://docs.bigmodel.cn/cn/coding-plan/faq
- Community GLM monitor reference (MIT): https://github.com/LaughSmiles/glm-key-monitor
- DeepSeek balance API: https://api-docs.deepseek.com/zh-cn/api/get-user-balance
- SiliconFlow user info API: https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info
- OpenRouter credits API: https://openrouter.ai/docs/api-reference/credits/get-credits
- Volcengine Ark usage API: https://www.volcengine.com/docs/82379/2116766
- Volcengine billing API overview: https://www.volcengine.com/docs/6269/1165275
- Alibaba Model Studio billing guide: https://help.aliyun.com/zh/model-studio/bill-query-and-cost-management
- OpenAI Usage API: https://platform.openai.com/docs/api-reference/usage
- Anthropic Claude Code Analytics API: https://platform.claude.com/docs/en/manage-claude/claude-code-analytics-api
- Anthropic Messages Usage Report API: https://platform.claude.com/docs/en/api/admin/usage_report
- xAI Management API guide: https://docs.x.ai/developers/management-api-guide
- xAI Management billing reference: https://docs.x.ai/developers/rest-api-reference/management/billing
- PPIO billing balance detail: https://ppio.com/docs/management/reference-billing-get-balance-detail
- PPIO FAQ (per-key usage not supported): https://ppio.com/docs/support/faq
- Gemini Code Assist monitoring: https://cloud.google.com/gemini/docs/codeassist/monitor-gemini-code-assist
- Alibaba Model Studio Prometheus monitoring: https://www.alibabacloud.com/help/en/model-studio/model-telemetry
- USD/CNY estimate source: https://frankfurter.dev/

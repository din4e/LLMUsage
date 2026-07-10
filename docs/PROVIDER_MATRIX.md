# Provider Capability Matrix

Last verified: 2026-07-10

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
| MiniMax China, Token Plan | Official API: remaining quota | Official API: plan remaining usage | Official API when reset fields are present; documented resource-limit error otherwise | Subscription usage, not per-call RMB cost | First-class online plan adapter |
| Kimi/Moonshot China API | Official API | Console only; daily billing may update the next morning | Response-derived for inference rate limits | Console only or response-derived estimate | Balance adapter plus optional observer |
| Kimi Code | Unverified public API | Console only | Console only; documented 5-hour rolling window and 7-day quota refresh | Included in subscription | Do not scrape; show setup limitation until a public API exists |
| GLM China API / Coding Plan | Community-verified API-key endpoints | Community-verified model and tool usage endpoints | Community-verified quota endpoint returns reset timestamps; official FAQ documents 5-hour and weekly limits | Included in subscription or response-derived estimate | Experimental online adapter with strict validation and graceful fallback |
| DeepSeek China API | Official API | Console only / response-derived | Response-derived | Response-derived estimate | Balance adapter plus optional observer |
| Volcengine Ark / Doubao China | Official billing API | Official `GetInferenceUsage` API | API/response-derived where available | Official billing API | First-class online usage and billing adapter |
| Alibaba Model Studio / Qwen China | Cloud billing API candidate; contract not yet verified | Console and bill data documented; public query contract not yet verified | Unverified | Billing data is generated with a documented delay | Keep disabled until Alibaba Cloud billing API contract and signing are tested |

## Official Endpoint Contracts Verified

### MiniMax China Token Plan

- Remaining-plan endpoint: `GET https://www.minimaxi.com/v1/token_plan/remains`
- Authentication uses the Token Plan key according to the official Token Plan FAQ.
- China and international Token Plan keys are separate products and must not share endpoint defaults.

### Kimi/Moonshot China API

- Balance endpoint: `GET https://api.moonshot.cn/v1/users/me/balance`
- Authentication: `Authorization: Bearer <MOONSHOT_API_KEY>`.
- Response exposes available, voucher, and cash balances.
- Official help describes daily per-model usage and cost in the console, but says daily billing is updated by 07:00 the following day. This is not a real-time public usage API.

### DeepSeek China API

- Balance endpoint: `GET https://api.deepseek.com/user/balance`
- Returns total available balance and balance components.
- The inference API is OpenAI compatible and returns usage for observed calls; account-wide daily usage remains a separate console capability unless a public endpoint is verified.

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

Requests send the BigModel API key directly in the `Authorization` header and accept optional `startTime` and `endTime` query parameters. The observed quota schema includes a plan `level` and a `limits` array whose entries contain `type`, `percentage`, `nextResetTime`, optional current usage, and optional per-model usage details. The current rolling token window is the `TOKENS_LIMIT` entry with the nearest reset time.

These endpoints are not currently documented in GLM's public official API reference. They therefore remain an **experimental compatibility source**, not an official API capability. Implementation requirements:

1. Validate the full response before persisting or displaying any value.
2. Never log the request headers or API key.
3. Apply a conservative refresh interval and exponential backoff.
4. On 401/403, delete no credentials and show an actionable authentication error.
5. On 404/schema drift, disable only online monitoring and retain response-derived/local data.
6. Identify the source in the UI as “兼容接口（非官方承诺）”.

## Plan Semantics Verified

### GLM Coding Plan

- Uses both a 5-hour limit and a weekly limit.
- Exhausted quota waits for the next window and does not fall through to normal account resources.
- Coding Plan endpoints differ from standard API endpoints.
- Public documentation points users to the web usage page; no public quota-query API has yet been verified.

### Kimi Code

- Uses a rolling 5-hour frequency window and a quota that refreshes every 7 days from the subscription start date.
- All logged-in devices and plan keys share the quota.
- Public documentation points users to the Kimi Code console; no public quota-query API has yet been verified.

## Security and UX Rules

1. Never accept session cookies, browser storage exports, or copied authorization requests.
2. Never label console-only or response-derived figures as official online usage.
3. Show the data source, observed interval, provider update delay, and last successful synchronization beside every metric.
4. Request cloud IAM permissions only for the exact usage/billing read actions needed by an adapter.
5. Store model keys and cloud access secrets only in the operating-system credential vault.
6. Region selection changes endpoints, currency, pricing, and credential namespace together.

## Research Queue

- GLM standard API balance and official publication/stability of the monitor endpoints.
- Kimi Code public quota endpoint.
- MiniMax international Token Plan endpoint and response schema.
- Alibaba Cloud Model Studio usage/billing API action names, signing, granularity, and delay.
- Tencent Hunyuan, Baidu Qianfan, SiliconFlow account-wide usage and billing APIs.
- OpenAI, Anthropic, Gemini, OpenRouter, Mistral, Groq, Together AI, and xAI organization usage/cost endpoints and required admin credentials.

## Official Sources

- MiniMax Token Plan FAQ: https://platform.minimaxi.com/docs/token-plan/faq
- MiniMax API overview: https://platform.minimaxi.com/docs/api-reference/api-overview
- Kimi balance API: https://platform.kimi.com/docs/api/balance
- Kimi balance and usage help: https://www.kimi.com/help/kimi-api/api-balance-and-usage
- Kimi Code benefits: https://www.kimi.com/zh-cn/help/kimi-code/benefits
- GLM Coding Plan FAQ: https://docs.bigmodel.cn/cn/coding-plan/faq
- Community GLM monitor reference (MIT): https://github.com/LaughSmiles/glm-key-monitor
- DeepSeek balance API: https://api-docs.deepseek.com/zh-cn/api/get-user-balance
- Volcengine Ark usage API: https://www.volcengine.com/docs/82379/2116766
- Volcengine billing API overview: https://www.volcengine.com/docs/6269/1165275
- Alibaba Model Studio billing guide: https://help.aliyun.com/zh/model-studio/bill-query-and-cost-management

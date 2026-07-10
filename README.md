# LLM Usage

Windows-first desktop dashboard for online LLM account usage, plan quota, balance, and RMB-facing cost/balance visibility.

## Current support

| Provider | Region | Source | Shows |
|---|---|---|---|
| GLM | China | Experimental API-key monitor endpoints | Today calls, tokens, rolling token-window percentage, reset countdown |
| Kimi Code / Moonshot API | China | Experimental Kimi Code usage endpoint with official Moonshot balance fallback | Every returned quota window, weekly usage, concurrency/total limits and reset times; or Moonshot CNY balance for an Open Platform key |
| Kimi | Global | Official balance endpoint | Available balance, same response contract as Moonshot balance |
| DeepSeek | China | Official balance endpoint | Available CNY balance, topped-up balance, granted balance |
| MiniMax | China | Official Token Plan remains endpoint with same-region host fallback | Every returned model/window with used, remaining, total, start/reset time and remaining duration |
| MiniMax | Global | Experimental Token Plan remains endpoint | Every returned model/window with complete quota counts and timing |
| SiliconFlow | China | Official user info endpoint | Available CNY balance, topped-up balance, free balance |
| SiliconFlow | Global | Official user info endpoint | Available balance with the same contract |
| OpenRouter | Global | Official credits endpoint | Remaining USD credits from total purchased minus usage |
| OpenAI / Codex API | Global | Official Organization Usage and Costs APIs | API requests, input/output tokens, model breakdown, USD cost and live-rate RMB estimate; requires Admin API Key |
| Claude Code | Global | Official Claude Code Analytics API | UTC daily sessions, tokens, model cost and development activity; requires Anthropic Admin API Key |
| Gemini Code Assist | Global | Google Cloud Monitoring | API calls and used tokens; requires Project ID, Monitoring Viewer and an explicit OAuth access token |
| Qwen / Model Studio | China / Global | Official private Prometheus monitoring | Per-model calls and token consumption; requires the monitoring HTTP API URL and a least-privilege AccessKey pair |

The app does not scrape web consoles, cookies, browser storage, prompts, responses, or authorization headers. API keys are encrypted with Windows DPAPI and stored per provider under the current Windows user. The auto-sync interval is a non-sensitive UI setting stored locally.

Only configured providers appear in the dashboard; all others stay in the Add Provider catalog. Every validated detail set is rendered in a native, keyboard-accessible disclosure that is collapsed by default. MiniMax retains percent-only `general` quotas as well as count-based video, image, speech, music, and model rows. Provider identity objects and raw API responses are deliberately excluded from snapshots and cache.

OpenAI organization data is not a claim about personal ChatGPT/Codex subscription quota. Claude Code personal Pro/Max remaining quota is not exposed by a public API. Qwen Coding Plan keys are not used for automated monitoring because the provider's terms prohibit custom automated clients; the supported Qwen path is the official private Prometheus monitoring API.

## Cost and balance policy

Official balance endpoints are displayed as balance, not guessed daily cost. Kimi Code, GLM Coding Plan and MiniMax Token Plan are subscription/quota views, so the app does not fabricate per-call RMB cost when the provider does not return it. OpenAI and Claude return USD cost; when the public exchange-rate query succeeds, the dashboard also shows a clearly estimated RMB total. Kimi Code keys and Moonshot Open Platform keys are different products; the China adapter detects the key family, contacts only the matching product endpoint, and labels the result accordingly.

## Development

```powershell
npm install
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

## Release build

```powershell
npm run tauri build
```

The release profile uses Tauri 2, native TypeScript, Rust `rustls`, stripping, and `panic = "abort"` to keep the Windows installer small. WebView2 is treated as a system runtime and is not counted in the package target.

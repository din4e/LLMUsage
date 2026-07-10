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

The app does not scrape web consoles, cookies, browser storage, prompts, responses, or authorization headers. API keys are encrypted with Windows DPAPI and stored per provider under the current Windows user. The auto-sync interval is a non-sensitive UI setting stored locally.

Kimi and MiniMax keep the compact provider summary and render all validated quota details directly underneath it. Provider identity objects and raw API responses are deliberately excluded from snapshots and cache.

## Cost and balance policy

Official balance endpoints are displayed as balance, not guessed daily cost. Kimi Code, GLM Coding Plan and MiniMax Token Plan are subscription/quota views, so the app does not fabricate per-call RMB cost when the provider does not return it. Kimi Code keys and Moonshot Open Platform keys are different products; the China adapter detects the key family, contacts only the matching product endpoint, and labels the result accordingly. The top metric uses the available RMB balance/cost signal from configured providers.

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

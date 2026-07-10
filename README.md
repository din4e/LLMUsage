# LLM Usage

Windows-first desktop dashboard for online LLM account usage, plan quota, balance, and RMB-facing cost/balance visibility.

## Current support

| Provider | Region | Source | Shows |
|---|---|---|---|
| GLM | China | Experimental API-key monitor endpoints | Today calls, tokens, rolling token-window percentage, reset countdown |
| Kimi | China | Official balance endpoint | Available CNY balance, cash balance, voucher balance |
| Kimi | Global | Official balance endpoint | Available balance, same response contract as Moonshot balance |
| DeepSeek | China | Official balance endpoint | Available CNY balance, topped-up balance, granted balance |
| MiniMax | China | Experimental Token Plan remains endpoint | Plan usage percentage and reset time when returned |
| MiniMax | Global | Experimental Token Plan remains endpoint | Plan usage percentage and reset time when returned |

The app does not scrape web consoles, cookies, browser storage, prompts, responses, or authorization headers. API keys are encrypted with Windows DPAPI and stored per provider under the current Windows user.

## Cost and balance policy

Official balance endpoints are displayed as balance, not guessed daily cost. GLM Coding Plan and MiniMax Token Plan are subscription/quota views, so the app does not fabricate per-call RMB cost when the provider does not return it. The top metric uses the available RMB balance/cost signal from configured providers.

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

The release profile uses Tauri 2, native TypeScript, Rust `rustls`, LTO, stripping, and `panic = "abort"` to keep the Windows installer small. WebView2 is treated as a system runtime and is not counted in the package target.

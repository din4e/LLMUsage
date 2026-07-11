# LLM Usage

面向 Windows 的桌面用量看板，用于汇总 LLM 在线账户的调用量、Token、订阅额度、余额及可估算的人民币成本。

应用只展示已配置的供应商；其余供应商会保留在「添加供应商」目录中。供应商图标使用本地打包的 [Lobe Icons](https://github.com/lobehub/lobe-icons) SVG，因此安装后无需联网加载图标。

## 支持的供应商

| 供应商 | 区域 | 数据来源 | 展示内容 |
| --- | --- | --- | --- |
| 智谱 GLM | 中国 | 社区 API Key 监控端点 | 当日调用、Token、滚动 Token 窗口百分比、重置倒计时 |
| Kimi Code / Moonshot API | 中国 | Kimi Code 用量端点；Moonshot 官方余额兜底 | 返回的全部额度窗口、周用量、并发/总量限制与重置时间；或开放平台人民币余额 |
| Kimi | 国际 | 官方余额接口 | 可用余额 |
| DeepSeek | 中国 | 官方余额接口 | 可用余额、充值余额、赠送余额 |
| MiniMax | 中国 | Token Plan 剩余额度接口，同区域主机兜底 | 所有模型和窗口的已用、剩余、总量、开始/重置时间与剩余时长 |
| MiniMax | 国际 | Token Plan 剩余额度接口 | 返回的全部资源额度及时间信息 |
| 硅基流动 | 中国 | 官方用户信息接口 | 可用人民币余额、充值余额、免费余额 |
| SiliconFlow | 国际 | 官方用户信息接口 | 可用余额 |
| OpenRouter | 国际 | 官方 Credits 接口 | 已购额度减去用量后的美元余额 |
| OpenAI / Codex API | 国际 | OpenAI Organization Usage 与 Costs API | 请求数、输入/输出 Token、模型明细、美元成本及人民币估算 |
| Claude Code | 国际 | Claude Code Analytics API | UTC 日汇总会话、Token、模型成本和开发活动 |
| Gemini Code Assist | 国际 | Google Cloud Monitoring | API 调用数和已用 Token |
| Qwen / Model Studio | 中国 / 国际 | 官方私有 Prometheus 监控 | 各模型调用数和 Token 消耗 |

## 凭据与数据边界

- 凭据按供应商分开保存，并使用当前 Windows 用户的 DPAPI 加密。
- 不读取网页控制台、Cookie、浏览器存储、聊天内容、提示词、响应内容或其他应用的凭据。
- 自动同步间隔只是本地非敏感设置。
- 在线返回的完整明细默认折叠，并支持键盘操作。
- MiniMax 会保留仅百分比的 `general` 额度，以及视频、图像、语音、音乐和模型等计数型额度。
- 原始 API 响应、账号身份信息不会写入快照或缓存。

不同产品的统计能力不同，应用会明确标注数据口径：

- OpenAI 显示的是 API 组织数据，需要 Organization Admin API Key；不等同于个人 ChatGPT/Codex 订阅剩余额度。
- Claude Code 需要 Anthropic Admin API Key；个人 Pro/Max 订阅没有公开的剩余额度 API。
- Gemini 需要 Google Cloud Project ID、Monitoring Viewer 权限，以及由用户主动提供的 OAuth Access Token；应用不会读取 gcloud 或浏览器凭据。
- Qwen 使用高级监控的 Prometheus HTTP API 和最小权限 AccessKey；不使用 Coding Plan Key 自动查询。
- Kimi Code Key、Moonshot 开放平台 Key，以及 MiniMax 国内/国际 Key 属于不同产品或区域，应用会按密钥类型匹配端点。

## 成本与余额规则

官方余额接口展示为余额，不会被猜测成当日成本。Kimi Code、GLM Coding Plan 和 MiniMax Token Plan 属于订阅/额度视图；当供应商未提供单次价格时，应用不会伪造人民币成本。

OpenAI 与 Claude 返回美元成本。若公开汇率查询可用，界面会额外显示清晰标注的人民币估算值。

## 开发

```powershell
npm install
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

## 构建 Windows 安装包

```powershell
npm run tauri build
```

安装包输出目录：

```text
src-tauri/target/release/bundle/nsis/
```

项目使用 Tauri 2、原生 TypeScript、Rust `rustls`、符号剥离和 `panic = "abort"` 控制安装包体积。WebView2 作为系统运行时，不计入安装包体积目标。

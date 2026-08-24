# Spec: Provider 最近变化量

## Objective

在仪表盘增加独立的「最近变化」面板。用户先单独选择一个 provider 实例，再选择一个指标，只查看该实例最近两条可比历史采样之间的变化量，不把多个 provider 或多个指标混在一起。

## Product Behavior

- Provider 必须按实例选择，例如 `GLM`、`GLM · 实例 2`；不提供“全部 provider”合计。
- 指标单选：请求、Token、余额、成本。
- 请求、Token、成本是当日累计量，只比较同一自然日内最近两条有效采样，避免跨日归零产生负数。
- 余额是存量，可跨日比较；变化量为 `最新余额 - 上次余额`，充值显示正数，消费显示负数。
- 显示当前值、变化量、两次采样时间；正数带 `+`，零显示 `0`，负数保留 `-`。
- Provider 不支持所选指标或不足两条可比采样时，显示明确空状态，不伪造 `0`。
- 数据继续来自现有非敏感 15 分钟历史记录；同一 15 分钟槽内重复同步只保留最新值，因此“最近”指最近两条持久化采样，不承诺逐次同步级变化。
- 已删除但仍有历史的实例不出现在默认可选列表；只展示当前已配置实例。

## Tech Stack and Commands

- TypeScript 7、原生 DOM、Vitest、Vite、Tauri 2。
- 测试：`npm test`
- 类型与生产构建：`npm run build`
- 桌面回归：`cargo test --manifest-path src-tauri/Cargo.toml -j 1`

## Project Structure

- `src/domain.ts`：选择最近两条可比采样并计算变化量的纯函数与类型。
- `src/domain.test.ts`：跨日、缺失指标、余额正负变化、多实例隔离测试。
- `index.html`：独立面板的语义结构和控件。
- `src/app.ts`：provider/指标选择状态与渲染。
- `src/styles.css`：沿用现有卡片、分段按钮和响应式视觉语言。
- `README.md`：v0.1.5 能力说明。

## Code Style

```ts
const change = selectLatestProviderChange(records, instanceId, metric);
if (!change) renderEmptyChange("至少需要两条可比采样");
else renderChange(change);
```

- 变化计算保持为无副作用纯函数；DOM 层只负责选择与展示。
- 不增加第三方依赖，不读取凭据或原始供应商响应。

## Testing Strategy

- RED：先为每种指标与边界条件写失败的领域单测。
- GREEN：实现最小变化选择函数。
- UI 使用现有原生控件模式，验证空状态、键盘操作、320px 窄屏布局与无控制台错误。
- 最后运行全部前端测试、生产构建和 Rust 回归测试。

## Boundaries

- Always：按实例隔离、只比较有效数字、保留正负号、空数据诚实降级。
- Ask first：改变 15 分钟历史粒度、增加新的持久化字段、将多个 provider 聚合。
- Never：把 Key、账号身份或原始响应写入历史；用缺失值补零；跨日比较累计指标。

## Success Criteria

1. 当前配置的每个 provider 实例都能被单独选择。
2. 请求、Token、余额、成本四个指标可以单独选择展示。
3. 至少两条可比采样时准确显示当前值、带符号变化量和采样时间。
4. 不支持或样本不足时显示空状态。
5. 重启后仍从已有历史恢复最近变化。
6. 全部测试和生产构建通过，版本保持 v0.1.5。

## Open Questions

- 是否按本规格保留四个指标；额度使用率暂不纳入，因为当前历史记录没有持久化该字段。

import anthropicLogo from "@lobehub/icons-static-svg/icons/anthropic.svg";
import claudeCodeLogo from "@lobehub/icons-static-svg/icons/claudecode-color.svg";
import codexLogo from "@lobehub/icons-static-svg/icons/codex-color.svg";
import deepSeekLogo from "@lobehub/icons-static-svg/icons/deepseek-color.svg";
import geminiLogo from "@lobehub/icons-static-svg/icons/gemini-color.svg";
import kimiLogo from "@lobehub/icons-static-svg/icons/kimi-color.svg";
import miniMaxLogo from "@lobehub/icons-static-svg/icons/minimax-color.svg";
import antigravityLogo from "@lobehub/icons-static-svg/icons/antigravity.svg";
import openRouterLogo from "@lobehub/icons-static-svg/icons/openrouter.svg";
import opencodeLogo from "@lobehub/icons-static-svg/icons/opencode.svg";
import ppioLogo from "@lobehub/icons-static-svg/icons/ppio-color.svg";
import qwenLogo from "@lobehub/icons-static-svg/icons/qwen-color.svg";
import siliconCloudLogo from "@lobehub/icons-static-svg/icons/siliconcloud-color.svg";
import xaiLogo from "@lobehub/icons-static-svg/icons/xai.svg";
import zhipuLogo from "@lobehub/icons-static-svg/icons/zhipu-color.svg";
import { baseProviderId } from "./domain";

export interface ProviderField {
  id: string;
  label: string;
  type: "text" | "password" | "url";
  placeholder: string;
  autocomplete?: string;
  /** Optional fields may stay empty; empty ones are dropped from the stored credential. */
  optional?: boolean;
}

export interface ProviderDefinition {
  id: string;
  name: string;
  subtitle: string;
  logo: string;
  credentialHint: string;
  fields: ProviderField[];
  /** OAuth providers authorize in the system browser instead of a form. */
  auth?: "grok" | "antigravity";
}

const apiKeyField = (label = "API Key", placeholder = "输入供应商 API Key"): ProviderField => ({
  id: "apiKey",
  label,
  type: "password",
  placeholder,
  autocomplete: "off",
});

export const providerDefinitions: ProviderDefinition[] = [
  {
    id: "glm",
    name: "智谱 GLM",
    subtitle: "Coding Plan · 兼容监控 · 团队版",
    logo: zhipuLogo,
    credentialHint: "需要订阅 GLM Coding Plan 的账号 API Key（监控 5 小时额度窗口）；普通按量付费 Key 无法查询，保存时会被拒绝。团队版请额外填写组织 ID（控制台团队页），项目 ID 可选。完整密钥仅交给 Rust 后端并由 Windows DPAPI 加密。",
    fields: [
      apiKeyField(),
      { id: "organization", label: "组织 ID（团队版选填）", type: "text", placeholder: "个人版留空", autocomplete: "off", optional: true },
      { id: "project", label: "项目 ID（可选）", type: "text", placeholder: "团队版默认项目可留空", autocomplete: "off", optional: true },
    ],
  },
  {
    id: "kimi_cn",
    name: "Kimi Code",
    subtitle: "中国 · 会员额度 / API 余额",
    logo: kimiLogo,
    credentialHint: "会员额度请使用 sk-kimi- Key；Moonshot 开放平台 Key 会自动查询 API 余额。",
    fields: [apiKeyField()],
  },
  {
    id: "kimi_global",
    name: "Kimi Global",
    subtitle: "国际 · API 余额",
    logo: kimiLogo,
    credentialHint: "使用 Moonshot AI 国际站 API Key。",
    fields: [apiKeyField()],
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    subtitle: "官方余额",
    logo: deepSeekLogo,
    credentialHint: "使用 DeepSeek API Platform Key；当前在线能力为官方余额。",
    fields: [apiKeyField()],
  },
  {
    id: "minimax_cn",
    name: "MiniMax 国内",
    subtitle: "Token Plan · 全资源额度",
    logo: miniMaxLogo,
    credentialHint: "使用 Token Plan 订阅 Key（通常以 sk-cp- 开头），普通按量 API Key 无法查询套餐额度。",
    fields: [apiKeyField()],
  },
  {
    id: "minimax_global",
    name: "MiniMax Global",
    subtitle: "Token Plan · All resources",
    logo: miniMaxLogo,
    credentialHint: "使用 MiniMax Global Token Plan Key；国内与国际 Key 不互通。",
    fields: [apiKeyField()],
  },
  {
    id: "siliconflow_cn",
    name: "硅基流动",
    subtitle: "中国 · 官方余额",
    logo: siliconCloudLogo,
    credentialHint: "使用 SiliconFlow 中国站 API Key。",
    fields: [apiKeyField()],
  },
  {
    id: "siliconflow_global",
    name: "SiliconFlow Global",
    subtitle: "International · Official balance",
    logo: siliconCloudLogo,
    credentialHint: "使用 SiliconFlow 国际站 API Key。",
    fields: [apiKeyField()],
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    subtitle: "Management credits",
    logo: openRouterLogo,
    credentialHint: "使用 OpenRouter Management Key 查询 purchased / used credits。",
    fields: [apiKeyField("Management Key")],
  },
  {
    id: "openai_codex",
    name: "OpenAI / Codex API",
    subtitle: "组织用量与成本 · 非 ChatGPT 套餐",
    logo: codexLogo,
    credentialHint: "需要 OpenAI Organization Admin API Key。统计 API 组织内的 Codex/模型用量与成本，不代表 ChatGPT 个人订阅剩余额度。",
    fields: [apiKeyField("Admin API Key", "sk-admin-…")],
  },
  {
    id: "claude_code",
    name: "Claude Code",
    subtitle: "官方日汇总 · Admin Analytics",
    logo: claudeCodeLogo,
    credentialHint: "需要 Anthropic Admin API Key（sk-ant-admin01-…）。个人 Pro/Max 套餐没有公开的剩余额度 API。",
    fields: [apiKeyField("Admin API Key", "sk-ant-admin01-…")],
  },
  {
    id: "anthropic_api",
    name: "Anthropic API",
    subtitle: "组织 Messages 用量 · Admin API",
    logo: anthropicLogo,
    credentialHint: "需要 Anthropic Admin API Key（sk-ant-admin01-…）。按模型统计组织内 Messages API 的输入、缓存与输出 Token；不包含余额或订阅额度。与 Claude Code 分析使用同一类密钥，可分别配置。",
    fields: [apiKeyField("Admin API Key", "sk-ant-admin01-…")],
  },
  {
    id: "xai",
    name: "xAI / Grok",
    subtitle: "Management API · 预付余额",
    logo: xaiLogo,
    credentialHint: "需要 xAI 控制台生成的 Management Key（Read 权限）与团队 ID；推理用 API Key（xai-…）无法查询余额。展示预付余额与今日消耗。",
    fields: [
      { id: "managementKey", label: "Management Key", type: "password", placeholder: "控制台 Settings 生成", autocomplete: "off" },
      { id: "teamId", label: "Team ID", type: "text", placeholder: "例如 1234567890", autocomplete: "off" },
    ],
  },
  {
    id: "ppio",
    name: "PPIO 派欧云",
    subtitle: "中国 · 官方余额",
    logo: ppioLogo,
    credentialHint: "使用 PPIO 开放平台 API Key（Bearer）。查询可用余额、现金余额与信用额度；用量需在控制台查看。",
    fields: [apiKeyField()],
  },
  {
    id: "gemini",
    name: "Gemini Code Assist",
    subtitle: "Google Cloud Monitoring",
    logo: geminiLogo,
    credentialHint: "需要 Monitoring Viewer 权限。OAuth Access Token 由你主动提供，应用不会读取 gcloud 或浏览器凭据；令牌过期后需重新配置。",
    fields: [
      { id: "projectId", label: "Google Cloud Project ID", type: "text", placeholder: "my-project" },
      { id: "accessToken", label: "OAuth Access Token", type: "password", placeholder: "ya29.…", autocomplete: "off" },
    ],
  },
  {
    id: "qwen_cn",
    name: "Qwen / 百炼国内",
    subtitle: "官方 Prometheus 模型监控",
    logo: qwenLogo,
    credentialHint: "需要已开启的百炼高级监控、其公网 Prometheus HTTP API 地址和最小权限 AccessKey。Coding Plan Key 不会用于自动查询。",
    fields: qwenFields(),
  },
  {
    id: "qwen_global",
    name: "Qwen / Model Studio Global",
    subtitle: "Official Prometheus monitoring",
    logo: qwenLogo,
    credentialHint: "使用国际站高级监控的公网 Prometheus HTTP API 地址及最小权限 AccessKey。",
    fields: qwenFields(),
  },
  {
    id: "opencode_go",
    name: "OpenCode Go",
    subtitle: "订阅额度 · 滚动/周/月窗口",
    logo: opencodeLogo,
    credentialHint: "使用 OpenCode Go 订阅的 API Key（opencode.ai/zen），查询滚动（5 小时）、周与月度额度窗口。",
    fields: [apiKeyField()],
  },
  {
    id: "grok",
    name: "Grok 订阅",
    subtitle: "SuperGrok · 周 Credits / 月度账单",
    logo: xaiLogo,
    credentialHint: "需要 SuperGrok 订阅。点击「授权并保存」后在系统浏览器登录 xAI 账号完成授权；应用只在本机回环端口接收授权码，令牌由 Windows DPAPI 加密保存。",
    fields: [],
    auth: "grok",
  },
  {
    id: "antigravity",
    name: "Google Antigravity",
    subtitle: "模型额度 · 滚动窗口与 AI Credits",
    logo: antigravityLogo,
    credentialHint: "需要 Google Antigravity 账号。点击「授权并保存」后在系统浏览器完成 Google 授权；项目 ID 自动识别，令牌由 Windows DPAPI 加密保存。",
    fields: [],
    auth: "antigravity",
  },
];

function qwenFields(): ProviderField[] {
  return [
    { id: "endpoint", label: "Prometheus HTTP API", type: "url", placeholder: "https://…aliyuncs.com" },
    { id: "accessKeyId", label: "AccessKey ID", type: "password", placeholder: "LTAI…", autocomplete: "off" },
    { id: "accessKeySecret", label: "AccessKey Secret", type: "password", placeholder: "输入最小权限 Secret", autocomplete: "off" },
  ];
}

export function providerDefinition(providerId: string): ProviderDefinition | undefined {
  const base = baseProviderId(providerId);
  return providerDefinitions.find((provider) => provider.id === base);
}

/** True when at least one instance of the provider is configured. */
export function hasConfiguredInstance(baseId: string, configured: ReadonlySet<string>): boolean {
  for (const instanceId of configured) {
    if (baseProviderId(instanceId) === baseId) return true;
  }
  return false;
}

/** The instance id to use when adding another account of `baseId`. */
export function nextInstanceId(baseId: string, configured: ReadonlySet<string>): string {
  let maxIndex = 0;
  for (const instanceId of configured) {
    if (baseProviderId(instanceId) === baseId) {
      maxIndex = Math.max(maxIndex, Number(instanceId.slice(baseId.length + 1)) || 1);
    }
  }
  return maxIndex === 0 ? baseId : `${baseId}_${maxIndex + 1}`;
}

/** Longest instance remark kept; longer input is truncated on save. */
export const INSTANCE_REMARK_MAX_LENGTH = 24;

/** Trims, collapses whitespace, and truncates a user-typed instance remark. */
export function sanitizeInstanceRemark(raw: string): string {
  const collapsed = raw.trim().replace(/\s+/g, " ");
  // Truncate by code points so a CJK or emoji remark is never cut mid-character.
  return Array.from(collapsed).slice(0, INSTANCE_REMARK_MAX_LENGTH).join("").trimEnd();
}

/** Badge text for an instance row: a remark wins over the "实例 N" fallback. */
export function instanceBadgeLabel(index: number, remark: string): string {
  if (remark) return remark;
  return index >= 2 ? `实例 ${index}` : "";
}

/** Full instance name used in dialog titles, statuses, and aria labels. */
export function instanceDisplayName(
  provider: ProviderDefinition,
  index: number,
  remark = "",
): string {
  if (remark) return `${provider.name} · ${remark}`;
  return index >= 2 ? `${provider.name} · 实例 ${index}` : provider.name;
}

export function serializeProviderCredential(
  providerId: string,
  values: Readonly<Record<string, string>>,
): string {
  const provider = providerDefinition(providerId);
  if (!provider) throw new Error("暂不支持该供应商");
  if (provider.auth) throw new Error("该供应商通过浏览器授权，无需填写凭据");

  const trimmed = Object.fromEntries(
    provider.fields.map((field) => [field.id, values[field.id]?.trim() ?? ""]),
  );
  if (provider.fields.some((field) => !field.optional && !trimmed[field.id])) {
    throw new Error("请填写所有必填项");
  }
  // Drop empty optional fields so the stored credential stays minimal; when
  // every optional field is empty the bare apiKey is stored exactly like a
  // single-field provider (older instances and backups keep working).
  const fields = Object.fromEntries(
    (Object.entries(trimmed) as [string, string][]).filter(([, value]) => value !== ""),
  );
  const apiKeyOnly = provider.fields.some((field) => field.id === "apiKey" && !field.optional);
  if (apiKeyOnly && Object.keys(fields).length === 1 && fields.apiKey !== undefined) {
    return fields.apiKey;
  }
  return JSON.stringify(fields);
}

/** Parses a stored credential back into per-field values for the edit dialog. */
export function deserializeProviderCredential(
  providerId: string,
  credential: string,
): Record<string, string> {
  const provider = providerDefinition(providerId);
  if (!provider) return {};
  const trimmed = credential.trim();
  if (!trimmed) return {};
  let values: Record<string, unknown>;
  if (trimmed.startsWith("{")) {
    try {
      values = JSON.parse(trimmed) as Record<string, unknown>;
    } catch {
      return {};
    }
  } else {
    // Single-field providers store the bare API key.
    values = { apiKey: trimmed };
  }
  const fields: Record<string, string> = {};
  for (const field of provider.fields) {
    if (typeof values[field.id] === "string") fields[field.id] = values[field.id] as string;
  }
  return fields;
}

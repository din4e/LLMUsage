import claudeCodeLogo from "@lobehub/icons-static-svg/icons/claudecode-color.svg";
import codexLogo from "@lobehub/icons-static-svg/icons/codex-color.svg";
import deepSeekLogo from "@lobehub/icons-static-svg/icons/deepseek-color.svg";
import geminiLogo from "@lobehub/icons-static-svg/icons/gemini-color.svg";
import kimiLogo from "@lobehub/icons-static-svg/icons/kimi-color.svg";
import miniMaxLogo from "@lobehub/icons-static-svg/icons/minimax-color.svg";
import openRouterLogo from "@lobehub/icons-static-svg/icons/openrouter.svg";
import qwenLogo from "@lobehub/icons-static-svg/icons/qwen-color.svg";
import siliconCloudLogo from "@lobehub/icons-static-svg/icons/siliconcloud-color.svg";
import zhipuLogo from "@lobehub/icons-static-svg/icons/zhipu-color.svg";

export interface ProviderField {
  id: string;
  label: string;
  type: "text" | "password" | "url";
  placeholder: string;
  autocomplete?: string;
}

export interface ProviderDefinition {
  id: string;
  name: string;
  subtitle: string;
  logo: string;
  credentialHint: string;
  fields: ProviderField[];
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
    subtitle: "Coding Plan · 兼容监控",
    logo: zhipuLogo,
    credentialHint: "使用智谱开放平台 API Key；完整密钥仅交给 Rust 后端并由 Windows DPAPI 加密。",
    fields: [apiKeyField()],
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
];

function qwenFields(): ProviderField[] {
  return [
    { id: "endpoint", label: "Prometheus HTTP API", type: "url", placeholder: "https://…aliyuncs.com" },
    { id: "accessKeyId", label: "AccessKey ID", type: "password", placeholder: "LTAI…", autocomplete: "off" },
    { id: "accessKeySecret", label: "AccessKey Secret", type: "password", placeholder: "输入最小权限 Secret", autocomplete: "off" },
  ];
}

export function providerDefinition(providerId: string): ProviderDefinition | undefined {
  return providerDefinitions.find((provider) => provider.id === providerId);
}

export function configuredProviders(configured: ReadonlySet<string>): ProviderDefinition[] {
  return providerDefinitions.filter((provider) => configured.has(provider.id));
}

export function unconfiguredProviders(configured: ReadonlySet<string>): ProviderDefinition[] {
  return providerDefinitions.filter((provider) => !configured.has(provider.id));
}

export function serializeProviderCredential(
  providerId: string,
  values: Readonly<Record<string, string>>,
): string {
  const provider = providerDefinition(providerId);
  if (!provider) throw new Error("暂不支持该供应商");

  const fields = Object.fromEntries(
    provider.fields.map((field) => [field.id, values[field.id]?.trim() ?? ""]),
  );
  if (Object.values(fields).some((value) => !value)) throw new Error("请填写所有必填项");
  if (provider.fields.length === 1 && provider.fields[0]?.id === "apiKey") return fields.apiKey ?? "";
  return JSON.stringify(fields);
}

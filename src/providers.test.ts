import { describe, expect, it } from "vitest";
import {
  INSTANCE_REMARK_MAX_LENGTH,
  deserializeProviderCredential,
  hasConfiguredInstance,
  instanceBadgeLabel,
  instanceDisplayName,
  nextInstanceId,
  providerDefinition,
  providerDefinitions,
  sanitizeInstanceRemark,
  serializeProviderCredential,
} from "./providers";

describe("provider catalog", () => {
  it("includes the requested coding and regional providers", () => {
    const ids = providerDefinitions.map((provider) => provider.id);

    expect(ids).toContain("openai_codex");
    expect(ids).toContain("claude_code");
    expect(ids).toContain("anthropic_api");
    expect(ids).toContain("xai");
    expect(ids).toContain("ppio");
    expect(ids).toContain("gemini");
    expect(ids).toContain("qwen_cn");
    expect(ids).toContain("qwen_global");
  });

  it("assigns each provider a local Lobe Icons SVG", () => {
    for (const provider of providerDefinitions) {
      expect(provider.logo).toMatch(/^(data:image\/svg\+xml|.*\.svg$)/);
      expect(provider.logo).not.toMatch(/^https?:/);
    }
  });

  it("uses the Zhipu brand icon for GLM", () => {
    expect(providerDefinitions.find((provider) => provider.id === "glm")?.logo).toContain("Zhipu");
  });

  it("tells GLM users the card requires a Coding Plan subscription", () => {
    const hint = providerDefinition("glm")?.credentialHint ?? "";

    expect(hint).toContain("Coding Plan");
    expect(hint).toContain("按量付费");
  });

  it("resolves definitions for base ids and numbered instances", () => {
    expect(providerDefinition("kimi_cn")?.id).toBe("kimi_cn");
    expect(providerDefinition("kimi_cn_2")?.id).toBe("kimi_cn");
    expect(providerDefinition("siliconflow_global_12")?.id).toBe("siliconflow_global");
    expect(providerDefinition("unknown_provider_2")).toBeUndefined();
  });

  it("detects configured instances of the same provider", () => {
    const configured = new Set(["glm", "glm_2", "kimi_cn"]);

    expect(hasConfiguredInstance("glm", configured)).toBe(true);
    expect(hasConfiguredInstance("kimi_cn", configured)).toBe(true);
    expect(hasConfiguredInstance("kimi_global", configured)).toBe(false);
    expect(hasConfiguredInstance("minimax_cn", new Set())).toBe(false);
  });

  it("allocates the next free instance id per provider", () => {
    expect(nextInstanceId("glm", new Set())).toBe("glm");
    expect(nextInstanceId("glm", new Set(["glm"]))).toBe("glm_2");
    expect(nextInstanceId("glm", new Set(["glm", "glm_2"]))).toBe("glm_3");
    expect(nextInstanceId("kimi_cn", new Set(["kimi_cn_2"]))).toBe("kimi_cn_3");
    expect(nextInstanceId("glm", new Set(["glm_2", "glm_10"]))).toBe("glm_11");
    expect(nextInstanceId("glm", new Set(["kimi_cn_2"]))).toBe("glm");
  });
});

describe("instance remarks", () => {
  it("sanitizes remark input into a compact single-line label", () => {
    expect(sanitizeInstanceRemark("  工作账号  ")).toBe("工作账号");
    expect(sanitizeInstanceRemark("工作\n\t 账号")).toBe("工作 账号");
    expect(sanitizeInstanceRemark("工作　账号")).toBe("工作 账号");
    expect(sanitizeInstanceRemark("   ")).toBe("");
    expect(sanitizeInstanceRemark("x".repeat(64))).toHaveLength(INSTANCE_REMARK_MAX_LENGTH);
  });

  it("prefers the remark for the instance badge", () => {
    expect(instanceBadgeLabel(3, "工作号")).toBe("工作号");
    expect(instanceBadgeLabel(1, "工作号")).toBe("工作号");
    expect(instanceBadgeLabel(3, "")).toBe("实例 3");
    expect(instanceBadgeLabel(1, "")).toBe("");
  });

  it("appends the remark to the full display name", () => {
    const kimi = providerDefinition("kimi_cn") ?? providerDefinitions[0]!;

    expect(instanceDisplayName(kimi, 1, "工作号")).toBe("Kimi Code · 工作号");
    expect(instanceDisplayName(kimi, 2, "工作号")).toBe("Kimi Code · 工作号");
    expect(instanceDisplayName(kimi, 2, "")).toBe("Kimi Code · 实例 2");
    expect(instanceDisplayName(kimi, 1, "")).toBe("Kimi Code");
  });
});

describe("provider credentials", () => {
  it("keeps simple API keys as a trimmed secret", () => {
    expect(serializeProviderCredential("minimax_cn", { apiKey: "  sk-cp-test  " })).toBe(
      "sk-cp-test",
    );
  });

  it("serializes credentials for numbered instances of the same provider", () => {
    expect(serializeProviderCredential("minimax_cn_2", { apiKey: "sk-cp-second" })).toBe(
      "sk-cp-second",
    );

    expect(
      JSON.parse(
        serializeProviderCredential("qwen_cn_2", {
          endpoint: "https://example.aliyuncs.com",
          accessKeyId: "LTAI-test",
          accessKeySecret: "secret-test",
        }),
      ),
    ).toEqual({
      endpoint: "https://example.aliyuncs.com",
      accessKeyId: "LTAI-test",
      accessKeySecret: "secret-test",
    });
  });

  it("serializes Gemini and Qwen multi-field credentials without dropping fields", () => {
    expect(
      JSON.parse(
        serializeProviderCredential("gemini", {
          projectId: "sample-project",
          accessToken: "ya29.test",
        }),
      ),
    ).toEqual({ projectId: "sample-project", accessToken: "ya29.test" });

    expect(
      JSON.parse(
        serializeProviderCredential("qwen_cn", {
          endpoint: "https://example.aliyuncs.com",
          accessKeyId: "LTAI-test",
          accessKeySecret: "secret-test",
        }),
      ),
    ).toEqual({
      endpoint: "https://example.aliyuncs.com",
      accessKeyId: "LTAI-test",
      accessKeySecret: "secret-test",
    });
  });

  it("serializes xAI management credentials as camelCase JSON", () => {
    expect(
      JSON.parse(
        serializeProviderCredential("xai", {
          managementKey: "xai-mgmt-test",
          teamId: "1234567890",
        }),
      ),
    ).toEqual({ managementKey: "xai-mgmt-test", teamId: "1234567890" });

    expect(() => serializeProviderCredential("xai", { managementKey: "xai-mgmt-test" })).toThrow(
      "请填写所有必填项",
    );
  });

  it("rejects incomplete credential forms", () => {
    expect(() => serializeProviderCredential("gemini", { projectId: "sample-project" })).toThrow(
      "请填写所有必填项",
    );
  });

  it("round-trips stored credentials back into dialog field values", () => {
    expect(deserializeProviderCredential("minimax_cn_2", "sk-cp-test")).toEqual({
      apiKey: "sk-cp-test",
    });

    const stored = serializeProviderCredential("qwen_cn", {
      endpoint: "https://example.aliyuncs.com",
      accessKeyId: "LTAI-test",
      accessKeySecret: "secret-test",
    });
    expect(deserializeProviderCredential("qwen_cn", stored)).toEqual({
      endpoint: "https://example.aliyuncs.com",
      accessKeyId: "LTAI-test",
      accessKeySecret: "secret-test",
    });
  });

  it("tolerates blank, corrupt, or partial stored credentials", () => {
    expect(deserializeProviderCredential("gemini", "")).toEqual({});
    expect(deserializeProviderCredential("gemini", "  ")).toEqual({});
    expect(deserializeProviderCredential("gemini", "{oops")).toEqual({});
    expect(deserializeProviderCredential("gemini", JSON.stringify({ projectId: "p" }))).toEqual({
      projectId: "p",
    });
    expect(deserializeProviderCredential("unknown_x", "abc")).toEqual({});
  });
});

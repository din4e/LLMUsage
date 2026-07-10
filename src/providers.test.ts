import { describe, expect, it } from "vitest";
import {
  configuredProviders,
  providerDefinitions,
  serializeProviderCredential,
  unconfiguredProviders,
} from "./providers";

describe("provider catalog", () => {
  it("includes the requested coding and regional providers", () => {
    const ids = providerDefinitions.map((provider) => provider.id);

    expect(ids).toContain("openai_codex");
    expect(ids).toContain("claude_code");
    expect(ids).toContain("gemini");
    expect(ids).toContain("qwen_cn");
    expect(ids).toContain("qwen_global");
  });

  it("keeps configured and unconfigured providers in separate views", () => {
    const configured = new Set(["glm", "minimax_cn", "gemini"]);

    expect(configuredProviders(configured).map((provider) => provider.id)).toEqual([
      "glm",
      "minimax_cn",
      "gemini",
    ]);
    expect(unconfiguredProviders(configured).some((provider) => provider.id === "glm")).toBe(false);
    expect(unconfiguredProviders(configured).some((provider) => provider.id === "qwen_cn")).toBe(true);
  });
});

describe("provider credentials", () => {
  it("keeps simple API keys as a trimmed secret", () => {
    expect(serializeProviderCredential("minimax_cn", { apiKey: "  sk-cp-test  " })).toBe(
      "sk-cp-test",
    );
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

  it("rejects incomplete credential forms", () => {
    expect(() => serializeProviderCredential("gemini", { projectId: "sample-project" })).toThrow(
      "请填写所有必填项",
    );
  });
});

import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // `tmp/` holds cloned reference repositories (e.g. sub2api) whose own
    // test files must not leak into this project's vitest runs.
    exclude: ["**/node_modules/**", "**/dist/**", "tmp/**"],
  },
});

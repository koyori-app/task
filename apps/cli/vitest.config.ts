import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/__tests__/*.test.ts"],
    globalSetup: ["src/__tests__/globalSetup.ts"],
    clearMocks: true,
    restoreMocks: true,
  },
});

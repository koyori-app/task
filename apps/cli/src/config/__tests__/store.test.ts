import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({ home: "" }));

vi.mock("node:os", () => ({
  default: { homedir: () => state.home, tmpdir: () => "/tmp" },
}));

describe("config store", () => {
  beforeEach(() => {
    state.home = fs.mkdtempSync(path.join(os.tmpdir(), "task-cli-test-"));
    vi.resetModules();
  });

  afterEach(() => {
    fs.rmSync(state.home, { recursive: true, force: true });
    vi.unstubAllEnvs();
  });

  it("reads and writes config only below a temporary home", async () => {
    const store = await import("../store");
    const expectedPath = path.join(state.home, ".config", "task", "config.yaml");

    expect(store.configPath()).toBe(expectedPath);
    expect(store.loadConfigFile()).toEqual({});

    store.saveConfigFile({
      api_url: "https://task.invalid/",
      token: "secret",
      tenant_id: "tenant-1",
    });

    expect(store.loadConfigFile()).toEqual({
      api_url: "https://task.invalid/",
      token: "secret",
      tenant_id: "tenant-1",
    });
    expect(fs.statSync(expectedPath).mode & 0o777).toBe(0o600);
  });

  it("prefers environment values and removes a trailing API slash", async () => {
    vi.stubEnv("TASK_API_URL", "https://api.invalid/");
    vi.stubEnv("TASK_TOKEN", "env-token");
    vi.stubEnv("TASK_TENANT", "env-tenant");
    const { resolveRuntimeConfig } = await import("../store");

    expect(resolveRuntimeConfig()).toEqual({
      api_url: "https://api.invalid",
      token: "env-token",
      tenant_id: "env-tenant",
    });
  });
});

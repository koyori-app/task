import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  createClient: vi.fn(() => ({ GET: vi.fn() })),
  resolveRuntimeConfig: vi.fn(() => ({
    api_url: "https://api.invalid",
    token: "token-1",
    tenant_id: "tenant-1",
  })),
}));

vi.mock("openapi-fetch", () => ({ default: mocks.createClient }));
vi.mock("../../config/store", () => ({
  resolveRuntimeConfig: mocks.resolveRuntimeConfig,
}));

describe("API client", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("creates one authenticated openapi-fetch client without a request", async () => {
    const { getClient } = await import("../client");

    expect(getClient()).toBe(getClient());
    expect(mocks.createClient).toHaveBeenCalledOnce();
    expect(mocks.createClient).toHaveBeenCalledWith({
      baseUrl: "https://api.invalid",
      headers: {
        Authorization: "Bearer token-1",
        Accept: "application/json",
      },
    });
  });

  it("caches the configured tenant id", async () => {
    const { getTenantId } = await import("../client");

    expect(getTenantId()).toBe("tenant-1");
    expect(getTenantId()).toBe("tenant-1");
    expect(mocks.resolveRuntimeConfig).toHaveBeenCalledOnce();
  });
});

import { describe, expect, expectTypeOf, it } from "vitest";
import type { ApiPaths, TaskListResponse, User } from "../paths";

// expectTypeOf assertions compile away at runtime, so `vitest run` alone cannot
// catch OpenAPI/type drift here. CI enforces this file via `pnpm typecheck:test`
// (tsc -p tsconfig.test.json) in .github/workflows/cli-test.yml.
describe("API path contract", () => {
  it("keeps core response and path parameter types wired", () => {
    expectTypeOf<
      ApiPaths["/v1/auth/me"]["get"]["responses"][200]["content"]["application/json"]
    >().toEqualTypeOf<User>();
    expectTypeOf<
      ApiPaths["/v1/tenants/{tenant_id}/projects/{project_id}/tasks"]["get"]["responses"][200]["content"]["application/json"]
    >().toEqualTypeOf<TaskListResponse>();
    expectTypeOf<
      ApiPaths["/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}"]["get"]["parameters"]["path"]
    >().toEqualTypeOf<{ tenant_id: string; project_id: string; id: string }>();
    expect(true).toBe(true);
  });
});

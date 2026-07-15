import { describe, expect, expectTypeOf, it } from "vitest";
import type { ApiPaths, TaskListResponse, User } from "../paths";

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

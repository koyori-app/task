import { Command } from "commander";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CliError, unwrapApiResult } from "../errors";
import { getOutputOptions } from "../command";
import { print } from "../output";
import { isUuid, parseTaskRef } from "../projects";
import { findDoneStatusId, resolveStatusId } from "../statuses";

const mocks = vi.hoisted(() => ({ GET: vi.fn() }));

vi.mock("../../api/client", () => ({
  getClient: () => ({ GET: mocks.GET }),
  getTenantId: () => "tenant-1",
}));

describe("CLI utilities", () => {
  beforeEach(() => {
    mocks.GET.mockResolvedValue({
      data: [
        { id: "todo", name: "Todo", is_done_state: false },
        { id: "done", name: "Complete", is_done_state: true },
      ],
      response: { status: 200 },
    });
  });

  it("prints stable JSON and human task output", () => {
    const log = vi.spyOn(console, "log").mockImplementation(() => undefined);
    print({ b: 2 }, { json: true });
    print({ seq_key: "APP-7", title: "Ship it" }, { json: false });
    expect(log).toHaveBeenNthCalledWith(1, '{\n  "b": 2\n}');
    expect(log).toHaveBeenNthCalledWith(2, "APP-7\tShip it");
  });

  it("unwraps API data and rejects an empty response", () => {
    expect(unwrapApiResult({ data: 7, response: new Response() })).toBe(7);
    expect(() => unwrapApiResult({ response: new Response() })).toThrow(
      new CliError("API returned empty response"),
    );
  });

  it("parses UUID and KEY-number task references", () => {
    const uuid = "00000000-0000-4000-8000-000000000001";
    expect(isUuid(uuid)).toBe(true);
    expect(parseTaskRef(uuid)).toEqual({ uuid });
    expect(parseTaskRef("TEAM-42")).toEqual({ projectKey: "TEAM", taskId: "TEAM-42" });
    expect(() => parseTaskRef("TEAM-nope")).toThrow("Invalid task reference");
  });

  it("finds named and done statuses through the expected API path", async () => {
    await expect(resolveStatusId("project-1", "todo")).resolves.toBe("todo");
    await expect(findDoneStatusId("project-1")).resolves.toBe("done");
    expect(mocks.GET).toHaveBeenCalledWith(
      "/v1/tenants/{tenant_id}/projects/{project_id}/statuses",
      { params: { path: { tenant_id: "tenant-1", project_id: "project-1" } } },
    );
  });

  it("inherits the root JSON output option", () => {
    const root = new Command().option("--json", "JSON", false);
    const child = root.command("child");
    root.parse(["node", "task", "--json", "child"]);
    expect(getOutputOptions(child)).toEqual({ json: true });
  });
});

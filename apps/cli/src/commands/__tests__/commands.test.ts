import { Command } from "commander";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { registerAuthCommands } from "../auth";
import { registerConfigCommands } from "../config";
import { registerMyCommands } from "../my";
import { registerProjectsCommands } from "../projects";
import { registerSprintsCommands } from "../sprints";
import { registerTasksCommands } from "../tasks";

const mocks = vi.hoisted(() => ({
  GET: vi.fn(),
  POST: vi.fn(),
  PUT: vi.fn(),
  DELETE: vi.fn(),
  loadConfigFile: vi.fn(() => ({} as Record<string, string>)),
  saveConfigFile: vi.fn(),
  print: vi.fn(),
  listProjects: vi.fn(),
  resolveProject: vi.fn(),
  resolveStatusId: vi.fn(),
  findDoneStatusId: vi.fn(),
}));

vi.mock("../../api/client", () => ({
  getClient: () => ({
    GET: mocks.GET,
    POST: mocks.POST,
    PUT: mocks.PUT,
    DELETE: mocks.DELETE,
  }),
  getTenantId: () => "tenant-1",
}));
vi.mock("../../config/store", () => ({
  configPath: () => "/tmp/task-cli-test/config.yaml",
  loadConfigFile: mocks.loadConfigFile,
  saveConfigFile: mocks.saveConfigFile,
}));
vi.mock("../../utils/command", () => ({
  getOutputOptions: () => ({ json: false }),
}));
vi.mock("../../utils/output", () => ({ print: mocks.print }));
vi.mock("../../utils/projects", () => ({
  isUuid: (value: string) => value === "00000000-0000-4000-8000-000000000001",
  listProjects: mocks.listProjects,
  parseTaskRef: (ref: string) => {
    const dash = ref.lastIndexOf("-");
    return { projectKey: ref.slice(0, dash), taskId: ref };
  },
  resolveProject: mocks.resolveProject,
}));
vi.mock("../../utils/statuses", () => ({
  resolveStatusId: mocks.resolveStatusId,
  findDoneStatusId: mocks.findDoneStatusId,
}));

function programWith(register: (program: Command) => void): Command {
  const program = new Command().exitOverride().option("--json", "JSON", false);
  register(program);
  return program;
}

describe("command registration and primary branches", () => {
  beforeEach(() => {
    mocks.GET.mockResolvedValue({ data: { id: "user-1" }, response: { status: 200 } });
    mocks.POST.mockResolvedValue({ data: { id: "task-1" }, response: { status: 201 } });
    mocks.listProjects.mockResolvedValue([{ id: "project-1", key: "APP", name: "App" }]);
    mocks.resolveProject.mockResolvedValue({ id: "project-1", key: "APP", name: "App" });
    mocks.resolveStatusId.mockResolvedValue("status-1");
  });

  it("auth parses whoami and calls the auth path", async () => {
    await programWith(registerAuthCommands).parseAsync(["node", "task", "auth", "whoami"]);
    expect(mocks.GET).toHaveBeenCalledWith("/v1/auth/me");
    expect(mocks.print).toHaveBeenCalledWith({ id: "user-1" }, { json: false });
  });

  it("config parses set and persists the selected key", async () => {
    await programWith(registerConfigCommands).parseAsync([
      "node", "task", "config", "set", "tenant_id", "tenant-2",
    ]);
    expect(mocks.saveConfigFile).toHaveBeenCalledWith({ tenant_id: "tenant-2" });
  });

  it("my parses list and sends its filter", async () => {
    await programWith(registerMyCommands).parseAsync([
      "node", "task", "my", "list", "--filter", "today",
    ]);
    expect(mocks.GET).toHaveBeenCalledWith(
      "/v1/tenants/{tenant_id}/users/me/tasks",
      expect.objectContaining({
        params: expect.objectContaining({ query: { filter: "today" } }),
      }),
    );
  });

  it("projects parses list and delegates project loading", async () => {
    await programWith(registerProjectsCommands).parseAsync(["node", "task", "projects", "list"]);
    expect(mocks.listProjects).toHaveBeenCalledOnce();
  });

  it("sprints parses list and builds tenant/project path parameters", async () => {
    mocks.GET.mockResolvedValueOnce({ data: [], response: { status: 200 } });
    await programWith(registerSprintsCommands).parseAsync([
      "node", "task", "sprints", "list", "--project", "APP", "--status", "active",
    ]);
    expect(mocks.GET).toHaveBeenCalledWith(
      "/v1/tenants/{tenant_id}/projects/{project_id}/sprints",
      {
        params: {
          path: { tenant_id: "tenant-1", project_id: "project-1" },
          query: { status: "active" },
        },
      },
    );
  });

  it("tasks parses create and resolves its optional status", async () => {
    await programWith(registerTasksCommands).parseAsync([
      "node", "task", "tasks", "create", "--project", "APP", "--title", "Golden task",
      "--priority", "medium", "--status", "Doing",
    ]);
    expect(mocks.resolveStatusId).toHaveBeenCalledWith("project-1", "Doing");
    expect(mocks.POST).toHaveBeenCalledWith(
      "/v1/tenants/{tenant_id}/projects/{project_id}/tasks",
      expect.objectContaining({
        body: { title: "Golden task", priority: "medium", status_id: "status-1" },
      }),
    );
  });
});

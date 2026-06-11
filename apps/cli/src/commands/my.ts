import { Command } from "commander";
import { getClient, getTenantId } from "../api/client";
import type { TaskPriority } from "../api/paths";
import { getOutputOptions } from "../utils/command";
import type { OutputOptions } from "../utils/output";
import { print } from "../utils/output";
import { unwrapApiResult } from "../utils/errors";
import { parseTaskRef, resolveProject } from "../utils/projects";
import { findDoneStatusId } from "../utils/statuses";

type MyCommandOptions = OutputOptions & {
  filter?: string;
  priority?: string;
};

export function registerMyCommands(program: Command): void {
  const my = program.command("my").description("My Tasks commands");

  my
    .command("list")
    .description("List tasks assigned to me")
    .option(
      "--filter <filter>",
      "today | week | no_due_date | overdue | all",
      "all",
    )
    .action(async (opts: MyCommandOptions, cmd) => {
      const output = getOutputOptions(cmd);
      const client = getClient();
      const tenantId = getTenantId();
      const result = await client.GET("/v1/tenants/{tenant_id}/users/me/tasks", {
        params: {
          path: { tenant_id: tenantId },
          query: { filter: opts.filter ?? "all" },
        },
      });
      print(unwrapApiResult(result), output);
    });

  my
    .command("add")
    .description("Quick-capture a task to personal inbox")
    .argument("<title>", "Task title")
    .option("--priority <priority>", "Task priority")
    .action(async (title: string, opts: MyCommandOptions, cmd) => {
      const output = getOutputOptions(cmd);
      const client = getClient();
      const tenantId = getTenantId();
      const body: {
        title: string;
        priority?: TaskPriority;
      } = { title };
      if (opts.priority) {
        body.priority = opts.priority as TaskPriority;
      }
      const result = await client.POST("/v1/tenants/{tenant_id}/users/me/tasks", {
        params: { path: { tenant_id: tenantId } },
        body,
      });
      print(unwrapApiResult(result), output);
    });

  my
    .command("complete")
    .description("Complete a personal or assigned task by ref (e.g. ME-3)")
    .argument("<ref>", "Task ref (ME-N, KEY-N, or UUID)")
    .action(async (ref: string, _opts, cmd) => {
      const output = getOutputOptions(cmd);
      const parsed = parseTaskRef(ref);
      const client = getClient();
      const tenantId = getTenantId();

      if ("uuid" in parsed) {
        const list = await client.GET("/v1/tenants/{tenant_id}/users/me/tasks", {
          params: {
            path: { tenant_id: tenantId },
            query: { filter: "all", limit: 200 },
          },
        });
        const tasks = unwrapApiResult(list).tasks;
        const task = tasks.find((item) => item.id === parsed.uuid);
        if (!task) {
          const { handleApiError } = await import("../utils/errors");
          handleApiError({ status: 404, message: `Task not found: ${ref}` });
        }
        const matchedTask = task!;
        const statusId = await findDoneStatusId(matchedTask.project.id);
        const result = await client.PUT(
          "/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}",
          {
            params: {
              path: {
                tenant_id: tenantId,
                project_id: matchedTask.project.id,
                id: matchedTask.id,
              },
            },
            body: { status_id: statusId },
          },
        );
        print(unwrapApiResult(result), output);
        return;
      }

      const project = await resolveProject(parsed.projectKey);
      const statusId = await findDoneStatusId(project.id);
      const result = await client.PUT(
        "/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}",
        {
          params: {
            path: {
              tenant_id: tenantId,
              project_id: project.id,
              id: parsed.taskId,
            },
          },
          body: { status_id: statusId },
        },
      );
      print(unwrapApiResult(result), output);
    });
}

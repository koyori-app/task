import path from "node:path";
import { spawnSync } from "node:child_process";
import { describe, expect, it } from "vitest";

const packageRoot = path.resolve(__dirname, "../..");
const entry = path.join(packageRoot, "bin", "task.js");

function run(args: string[]) {
  return spawnSync(process.execPath, [entry, ...args], {
    cwd: packageRoot,
    encoding: "utf8",
    env: { ...process.env, NO_COLOR: "1" },
  });
}

describe("bin/task.js entrypoint", () => {
  it("prints help and package version through the compiled index", () => {
    const help = run(["--help"]);
    expect(help.status).toBe(0);
    expect(help.stdout).toContain("Task management CLI");
    expect(help.stdout).toContain("auth");
    expect(help.stdout).toContain("tasks");

    const version = run(["--version"]);
    expect(version.status).toBe(0);
    expect(version.stdout.trim()).toBe("0.1.0");
  });

  it.each(["auth", "config", "my", "projects", "sprints", "tasks"])(
    "starts the %s command without API or config access",
    (command) => {
      const result = run([command, "--help"]);
      expect(result.status).toBe(0);
      expect(result.stdout).toContain(`Usage: task ${command}`);
      expect(result.stderr).toBe("");
    },
  );
});

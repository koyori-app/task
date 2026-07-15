import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const packageRoot = path.resolve(__dirname, "../..");
const compiledEntry = path.join(packageRoot, "dist", "index.js");

export default function globalSetup() {
  if (fs.existsSync(compiledEntry)) {
    return;
  }
  execSync("pnpm run build", { cwd: packageRoot, stdio: "inherit" });
}

import { execFileSync } from 'node:child_process';
import path from 'node:path';
import type { FullConfig } from '@playwright/test';

const DEFAULT_DB_URL = 'postgresql://test:test@localhost:5432/task_e2e';

/** Apply backend migration crate schema before Playwright webServers start. */
export default async function globalSetup(_config: FullConfig) {
  const databaseUrl = process.env.E2E_DATABASE_URL ?? DEFAULT_DB_URL;
  const env = {
    ...process.env,
    DATABASE_URL: databaseUrl,
  };

  if (process.env.MIGRATION_BIN) {
    execFileSync(process.env.MIGRATION_BIN, ['up'], {
      env,
      stdio: 'inherit',
    });
    return;
  }

  const migrationDir = path.resolve(import.meta.dirname, '../apps/backend/migration');
  execFileSync('cargo', ['run', '--', 'up'], {
    cwd: migrationDir,
    env,
    stdio: 'inherit',
  });
}

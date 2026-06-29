import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from 'pg';

const DEFAULT_DB_URL = 'postgresql://test:test@localhost:5432/task_e2e';

function readEnvFile(path: string): Record<string, string> {
  if (!existsSync(path)) {
    return {};
  }

  return Object.fromEntries(
    readFileSync(path, 'utf8')
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith('#') && line.includes('='))
      .map((line) => {
        const index = line.indexOf('=');
        const key = line.slice(0, index).trim();
        const rawValue = line.slice(index + 1).trim();
        return [key, rawValue.replace(/^['"]|['"]$/g, '')];
      }),
  );
}

function resolveDatabaseUrl() {
  const rootEnv = readEnvFile(resolve(import.meta.dirname, '../../.env'));
  const e2eEnv = readEnvFile(resolve(import.meta.dirname, '../.env'));

  return (
    process.env.E2E_DATABASE_URL ??
    e2eEnv.E2E_DATABASE_URL ??
    rootEnv.E2E_DATABASE_URL ??
    process.env.DATABASE_URL ??
    e2eEnv.DATABASE_URL ??
    rootEnv.DATABASE_URL ??
    DEFAULT_DB_URL
  );
}

export async function setEmailVerified(email: string, dbUrl: string): Promise<number> {
  const db = new Client({ connectionString: dbUrl });
  await db.connect();

  try {
    const result = await db.query('UPDATE users SET email_verified = true WHERE email = $1', [email]);
    return result.rowCount ?? 0;
  } finally {
    await db.end();
  }
}

async function main() {
  const email = process.argv[2];

  if (!email) {
    console.error('Usage: bun e2e/scripts/verify-email.ts <email>');
    process.exitCode = 1;
    return;
  }

  const updatedRows = await setEmailVerified(email, resolveDatabaseUrl());
  console.log(updatedRows);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main();
}

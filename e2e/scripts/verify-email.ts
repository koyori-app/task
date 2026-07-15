import { fileURLToPath } from 'node:url';
import { Client } from 'pg';
import { resolveE2eDatabaseUrl } from '../env';

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

  const updatedRows = await setEmailVerified(email, resolveE2eDatabaseUrl());
  console.log(updatedRows);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main();
}

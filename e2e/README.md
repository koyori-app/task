# E2E Notes

E2e PostgreSQL URLs resolve through a single precedence chain shared by TypeScript (`e2e/env.ts`) and shell (`e2e/scripts/resolve-database-url.sh`):

1. `E2E_DATABASE_URL` or `DATABASE_URL` from the process environment
2. The same keys from `e2e/.env`, repo `.env`, then `apps/backend/.env`
3. `database_url` from `apps/backend/.env` (backend convention)
4. Default `postgresql://test:test@localhost:5432/task_e2e`

`scripts/verify-email.sh` and `e2e/scripts/start-backend.sh` source the shell helper; Playwright specs import `resolveE2eDatabaseUrl()` from `e2e/env.ts`.

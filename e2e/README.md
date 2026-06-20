# E2E Notes

`scripts/verify-email.sh` keeps CI-provided `E2E_DATABASE_URL` or `DATABASE_URL` unchanged. When neither variable is set, it reads `apps/backend/.env` and exports `database_url` as `DATABASE_URL` if present.

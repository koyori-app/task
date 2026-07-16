declare const tenantUuidBrand: unique symbol;
declare const projectUuidBrand: unique symbol;

/** Tenant UUID resolved from API data, never a route display ID. */
export type TenantUuid = string & { readonly [tenantUuidBrand]: true };

/** Project UUID resolved from API data, never a route project key. */
export type ProjectUuid = string & { readonly [projectUuidBrand]: true };

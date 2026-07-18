import { computed, type MaybeRefOrGetter, toValue } from 'vue';
import type { TenantUuid } from '@/lib/api-ids';
import type { Tenant } from '@/stores/tenant';

/** Resolve route tenant slug (display_id or UUID) to tenant UUID from loaded tenants. */
export function resolveTenantIdFromRoute(
  tenants: Tenant[],
  routeTenantSlug: string,
): TenantUuid | null {
  if (!routeTenantSlug) return null;
  const id =
    tenants.find((tenant) => tenant.display_id === routeTenantSlug || tenant.id === routeTenantSlug)
      ?.id ?? null;
  return id ? (id as TenantUuid) : null;
}

/** Tenant id aligned to the current route — never falls back to persisted selectedTenantId. */
export function useRouteAlignedTenantId(
  tenants: MaybeRefOrGetter<Tenant[]>,
  routeTenantSlug: MaybeRefOrGetter<string>,
) {
  return computed(() =>
    resolveTenantIdFromRoute(toValue(tenants), String(toValue(routeTenantSlug) ?? '')),
  );
}

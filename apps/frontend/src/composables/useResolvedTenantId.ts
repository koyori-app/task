import { computed, type MaybeRefOrGetter, toValue } from 'vue';
import { useQuery } from '@tanstack/vue-query';

import { fetchClient } from '@/lib/api-vue-query';

const LIST_TENANTS_PATH = '/v1/tenants' as const;

/** Route param (display_id) を GET /v1/tenants で UUID に解決する。 */
export function useResolvedTenantId(tenantDisplayId: MaybeRefOrGetter<string>) {
  const displayId = computed(() => String(toValue(tenantDisplayId) ?? ''));

  const tenantsQuery = useQuery({
    queryKey: ['get', LIST_TENANTS_PATH],
    queryFn: async ({ signal }) => {
      const { data, error } = await fetchClient.GET(LIST_TENANTS_PATH, { signal });
      if (error) throw error;
      return data;
    },
    enabled: computed(() => !!displayId.value),
    staleTime: 60_000,
  });

  const tenantId = computed(() => {
    const data = tenantsQuery.data.value;
    if (!data || !displayId.value) return null;
    const tenants = Array.isArray(data) ? data : data.tenants;
    return tenants.find((t) => t.display_id === displayId.value)?.id ?? null;
  });

  const isTenantNotFound = computed(
    () =>
      !!displayId.value &&
      tenantsQuery.isSuccess.value &&
      !tenantsQuery.isFetching.value &&
      tenantId.value === null,
  );

  const isResolving = computed(() => tenantsQuery.isLoading.value);

  return {
    tenantDisplayId: displayId,
    tenantId,
    isTenantNotFound,
    isResolving,
    tenantsQuery,
  };
}

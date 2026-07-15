import { computed, type MaybeRefOrGetter, toValue } from 'vue';
import { useQuery } from '@tanstack/vue-query';

import { fetchClient } from '@/lib/api-vue-query';

const LIST_PROJECTS_PATH = '/v1/tenants/{tenant_id}/projects' as const;

/** Route param (projectKey) をテナント配下の project UUID に解決する。 */
export function useResolvedProjectId(
  tenantId: MaybeRefOrGetter<string | null | undefined>,
  projectKey: MaybeRefOrGetter<string>,
) {
  const resolvedTenantId = computed(() => toValue(tenantId) ?? null);
  const resolvedProjectKey = computed(() => String(toValue(projectKey) ?? ''));

  const projectsQuery = useQuery({
    queryKey: computed(() => [
      'get',
      LIST_PROJECTS_PATH,
      { params: { path: { tenant_id: resolvedTenantId.value! } } },
    ]),
    queryFn: async ({ signal }) => {
      const { data, error } = await fetchClient.GET(LIST_PROJECTS_PATH, {
        params: { path: { tenant_id: resolvedTenantId.value! } },
        signal,
      });
      if (error) throw error;
      return data;
    },
    enabled: computed(() => !!resolvedTenantId.value && !!resolvedProjectKey.value),
  });

  const projectId = computed(() => {
    const projects = projectsQuery.data.value;
    if (!projects || !resolvedProjectKey.value) return null;
    return projects.find((project) => project.key === resolvedProjectKey.value)?.id ?? null;
  });

  const isProjectNotFound = computed(
    () =>
      !!resolvedProjectKey.value &&
      projectsQuery.isSuccess.value &&
      !projectsQuery.isFetching.value &&
      projectId.value === null,
  );

  return {
    projectId,
    isProjectNotFound,
    isResolving: computed(() => projectsQuery.isLoading.value),
    isError: computed(() => projectsQuery.isError.value),
    error: computed(() => projectsQuery.error.value),
    projectsQuery,
  };
}

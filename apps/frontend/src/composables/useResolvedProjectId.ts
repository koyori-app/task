import { computed, type MaybeRefOrGetter, toValue } from 'vue';

import { useProjectsQuery } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

/** Route param (projectKey) をテナント配下の project UUID に解決する。 */
export function useResolvedProjectId(
  tenantId: MaybeRefOrGetter<string | null | undefined>,
  projectKey: MaybeRefOrGetter<string>,
) {
  const resolvedTenantId = computed(() => toValue(tenantId) ?? null);
  const resolvedProjectKey = computed(() => String(toValue(projectKey) ?? ''));

  const projectsQuery = useProjectsQuery(resolvedTenantId);

  const projectId = computed(() => {
    const projects = projectsQuery.data.value;
    if (!projects || !resolvedProjectKey.value) return null;
    return (
      projects.find((project: ProjectResponse) => project.key === resolvedProjectKey.value)?.id ??
      null
    );
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

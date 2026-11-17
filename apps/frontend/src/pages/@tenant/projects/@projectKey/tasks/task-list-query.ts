import type { PaginationState } from '@tanstack/vue-table';
export type TasksListQueryKeyParams = {
  params: {
    path: { tenant_id: string; project_id: string };
    query: { limit: number; offset: number; q?: string; timezone?: string };
  };
};
type PreviousTasksQuery = { queryKey: readonly unknown[] };
export function buildTasksListQueryParams(
  tenantId: string,
  projectId: string,
  pagination: PaginationState,
  q: string,
  timezone?: string,
): TasksListQueryKeyParams {
  return {
    params: {
      path: { tenant_id: tenantId, project_id: projectId },
      query: {
        limit: pagination.pageSize,
        offset: pagination.pageIndex * pagination.pageSize,
        q: q || undefined,
        timezone: q ? timezone : undefined,
      },
    },
  };
}

export function taskListPlaceholderData<T>(
  previousData: T | undefined,
  previousQuery: PreviousTasksQuery | undefined,
  currentProjectId: string | null,
  q: string,
  _timezone?: string,
): T | undefined {
  const previousParams = previousQuery?.queryKey[2] as TasksListQueryKeyParams | undefined;
  const previousProjectId = previousParams?.params.path.project_id;
  const previousQ = previousParams?.params.query.q ?? '';
  if (
    previousProjectId &&
    currentProjectId &&
    previousProjectId === currentProjectId &&
    previousQ === q
  ) {
    return previousData;
  }
  return undefined;
}

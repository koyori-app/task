import createFetchClient from 'openapi-fetch';
import { createClient } from '@koyori-app/openapi-vue-query';
import type { paths } from '@/generated/api';

export const fetchClient = createFetchClient<paths>({
  baseUrl: import.meta.env.VITE_API_BASE ?? '/api',
  fetch: (req: Request) => globalThis.fetch(req),
});

/** Typed TanStack Vue Query helpers for task OpenAPI paths. */
export const apiClient = createClient<paths>(fetchClient);

export const meQueryOptions = () => apiClient.queryOptions('get', '/v1/auth/me');

export const projectLabelsQueryOptions = (tenantId: string, projectId: string) =>
  apiClient.queryOptions('get', '/v1/tenants/{tenant_id}/projects/{project_id}/labels', {
    params: { path: { tenant_id: tenantId, project_id: projectId } },
  });

export function useMeQuery() {
  return apiClient.useQuery('get', '/v1/auth/me');
}

export function useLogoutMutation() {
  return apiClient.useMutation('post', '/v1/auth/logout');
}

export function createTestApiClient(fetchImpl: (input: Request) => Promise<Response>) {
  return createClient<paths>(
    createFetchClient<paths>({
      baseUrl: 'http://test.local/api',
      fetch: fetchImpl,
    }),
  );
}

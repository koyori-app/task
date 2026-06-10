import createFetchClient from 'openapi-fetch';
import { createClient as createOpenApiVueQueryClient } from '@koyori-app/openapi-vue-query';
import type { paths } from '@/generated/api';

const fetchClient = createFetchClient<paths>({
  baseUrl: import.meta.env.VITE_API_BASE ?? '/api',
});

type GetPaths = {
  [Path in keyof paths]: paths[Path] extends { get: unknown } ? Path : never;
}[keyof paths];

type PostPaths = {
  [Path in keyof paths]: paths[Path] extends { post: unknown } ? Path : never;
}[keyof paths];

const _mePathTypeProof: '/v1/auth/me' extends GetPaths ? true : false = true;
const _logoutPathTypeProof: '/v1/auth/logout' extends PostPaths ? true : false = true;

/** Typed TanStack Vue Query helpers for task OpenAPI paths. */
export const apiVueQueryClient = createOpenApiVueQueryClient(fetchClient);

const useTypedQuery = apiVueQueryClient.useQuery as <
  Path extends GetPaths,
  Init extends Record<string, unknown> | undefined = undefined,
>(
  method: 'get',
  path: Path,
  ...args: Init extends undefined ? [Init?, object?] : [Init, object?]
) => ReturnType<typeof apiVueQueryClient.useQuery>;

const useTypedMutation = apiVueQueryClient.useMutation as <Path extends PostPaths>(
  method: 'post',
  path: Path,
  ...args: [object?]
) => ReturnType<typeof apiVueQueryClient.useMutation>;

const queryOptionsFor = apiVueQueryClient.queryOptions as <
  Path extends GetPaths,
  Init extends Record<string, unknown> | undefined = undefined,
>(
  method: 'get',
  path: Path,
  ...args: Init extends undefined ? [Init?, object?] : [Init, object?]
) => ReturnType<typeof apiVueQueryClient.queryOptions>;

export const meQueryOptions = () => queryOptionsFor('get', '/v1/auth/me');

export const projectLabelsQueryOptions = (tenantId: string, projectId: string) =>
  queryOptionsFor('get', '/v1/tenants/{tenant_id}/projects/{project_id}/labels', {
    params: { path: { tenant_id: tenantId, project_id: projectId } },
  });

export function useMeQuery() {
  return useTypedQuery('get', '/v1/auth/me');
}

export function useLogoutMutation() {
  return useTypedMutation('post', '/v1/auth/logout');
}

export function createTestApiVueQueryClient(fetchImpl: (input: Request) => Promise<Response>) {
  const client = createOpenApiVueQueryClient(
    createFetchClient<paths>({
      baseUrl: 'http://test.local/api',
      fetch: fetchImpl,
    }),
  );

  const useMeQueryForTest = client.useQuery as typeof useTypedQuery;
  const useLogoutMutationForTest = client.useMutation as typeof useTypedMutation;

  return {
    useMeQuery: () => useMeQueryForTest('get', '/v1/auth/me'),
    useLogoutMutation: () => useLogoutMutationForTest('post', '/v1/auth/logout'),
  };
}

export type { GetPaths, PostPaths };
void _mePathTypeProof;
void _logoutPathTypeProof;

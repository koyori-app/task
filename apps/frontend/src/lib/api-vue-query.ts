import createFetchClient, { type Middleware } from 'openapi-fetch';
import { createClient } from '@koyori-app/openapi-vue-query';
import { computed, ref, toValue, type MaybeRefOrGetter } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import type { paths } from '@/generated/api';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';

/** Session /me cache duration — auth source of truth refresh interval. */
export const AUTH_ME_STALE_TIME_MS = 60_000;
export const TASK_SEARCH_STALE_TIME_MS = 30_000;

export const LIST_PROJECTS_PATH = '/v1/tenants/{tenant_id}/projects' as const;
export const TASK_SEARCH_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search' as const;

export const fetchClient = createFetchClient<paths>({
  baseUrl: import.meta.env.VITE_API_BASE ?? '/api',
  fetch: (req: Request) => globalThis.fetch(req),
  credentials: 'include',
});

/** セッション切れを伝えてから /signin へ送るまでの待ち時間。 */
export const SESSION_EXPIRED_REDIRECT_DELAY_MS = 2_000;

/** 操作中の 401 でセッション切れを検知したか。レイアウトが「セッションが切れました」を出す。 */
export const sessionExpired = ref(false);

/**
 * 操作中の API が「セッションが無い」401 を返したら、表示を出して /signin へ送る。
 * ログイン失敗（invalid-credentials / invalid-2fa-code）や、/me 自身（useAuthSession が扱う）は
 * 本文で区別して対象外にする。
 */
export const sessionExpiredMiddleware: Middleware = {
  async onResponse({ request, response }) {
    if (response.status !== 401) return undefined;
    if (new URL(request.url).pathname.endsWith('/v1/auth/me')) return undefined;
    const body = (await response
      .clone()
      .json()
      .catch(() => null)) as { message?: unknown } | null;
    if (body?.message !== 'unauthorized') return undefined;
    if (typeof window === 'undefined' || sessionExpired.value) return undefined;
    if (['/signin', '/signup'].includes(window.location.pathname)) return undefined;
    sessionExpired.value = true;
    setTimeout(() => window.location.assign('/signin'), SESSION_EXPIRED_REDIRECT_DELAY_MS);
    return undefined;
  },
};

fetchClient.use(sessionExpiredMiddleware);

/** HTTP 状態。応答が無い（通信断）なら undefined。 */
export function queryErrorStatus(error: unknown): number | undefined {
  return (error as { response?: Response } | null)?.response?.status;
}

/** /me の再試行。401・403（半認証）は確定なので再試行せず、5xx・通信断だけ 2 回まで。 */
export const ME_RETRY_COUNT = 2;
export function meRetry(failureCount: number, error: unknown): boolean {
  const status = queryErrorStatus(error);
  return status !== 401 && status !== 403 && failureCount < ME_RETRY_COUNT;
}
export const meRetryDelay = (attempt: number) => Math.min(1000 * 2 ** attempt, 10_000);

/** Typed TanStack Vue Query helpers for task OpenAPI paths. */
export const apiClient = createClient<paths>(fetchClient);

export const meQueryOptions = () =>
  apiClient.queryOptions('get', '/v1/auth/me', undefined, {
    staleTime: AUTH_ME_STALE_TIME_MS,
    retry: false,
  });

export const projectLabelsQueryOptions = (
  tenantId: TenantUuid | null | undefined,
  projectId: ProjectUuid | null | undefined,
) =>
  apiClient.queryOptions('get', '/v1/tenants/{tenant_id}/projects/{project_id}/labels', {
    params: { path: { tenant_id: tenantId ?? '', project_id: projectId ?? '' } },
  });

export const projectsQueryOptions = (tenantId: TenantUuid | null | undefined) =>
  apiClient.queryOptions(
    'get',
    LIST_PROJECTS_PATH,
    { params: { path: { tenant_id: tenantId ?? '' } } },
    { staleTime: AUTH_ME_STALE_TIME_MS },
  );

export const taskSearchQueryOptions = (
  tenantId: string,
  projectId: string,
  query: string,
  pagination: { limit?: number; offset?: number } = {},
) =>
  apiClient.queryOptions(
    'get',
    TASK_SEARCH_PATH,
    {
      params: {
        path: { tenant_id: tenantId, project_id: projectId },
        query: { q: query, ...pagination },
      },
    },
    { staleTime: TASK_SEARCH_STALE_TIME_MS, retry: false },
  );

export function useProjectsQuery(tenantId: MaybeRefOrGetter<TenantUuid | null | undefined>) {
  const resolvedTenantId = computed(() => toValue(tenantId) ?? null);

  return useQuery(
    computed(() => {
      const id = resolvedTenantId.value;
      return {
        ...projectsQueryOptions(id),
        enabled: !!id,
      };
    }),
  );
}

const ASSIGNABLE_USERS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/assignable-users' as const;

/**
 * タスクの担当者に指定できる利用者。
 *
 * メンバー一覧（`/members`）は使わない。あちらはメンバー管理の口で
 * プロジェクト管理者しか読めず、担当者を触れる人が候補だけ 403 になる。
 * メンバー指定のない共有プロジェクトはテナント全体へ開放されるため、返る集合も違う。
 */
export function useAssignableUsersQuery(
  // 表示用 ID をそのまま渡させない（api-path-params の規則）。解決済みの UUID を受ける
  tenantId: MaybeRefOrGetter<TenantUuid | null | undefined>,
  projectId: MaybeRefOrGetter<ProjectUuid | null | undefined>,
) {
  return useQuery(
    computed(() => {
      const tenantUuid = toValue(tenantId);
      const projectUuid = toValue(projectId);
      // 非 null は enabled で担保する（未解決のうちは問い合わせない）
      const params = {
        params: { path: { tenant_id: tenantUuid!, project_id: projectUuid! } },
      };
      return {
        queryKey: ['get', ASSIGNABLE_USERS_PATH, params],
        queryFn: async ({ signal }: { signal: AbortSignal }) => {
          const { data, error } = await fetchClient.GET(ASSIGNABLE_USERS_PATH, {
            ...params,
            signal,
          });
          if (error) throw error;
          return data;
        },
        enabled: !!tenantUuid && !!projectUuid,
      };
    }),
  );
}

const USER_PROFILE_PATH = '/v1/tenants/{tenant_id}/users/{user_id}' as const;

/**
 * テナント内の利用者のプロフィール。見つからなければ `null`。
 *
 * テナントに関わらない利用者も 404 で返る（存在を明かさない）。
 * それ以外の失敗は `status` を付けて投げる（閲覧者が客分なら 403）。
 */
export function useUserProfileQuery(
  tenantId: MaybeRefOrGetter<TenantUuid | null | undefined>,
  userId: MaybeRefOrGetter<string>,
) {
  return useQuery(
    computed(() => {
      const tenantUuid = toValue(tenantId);
      const userUuid = toValue(userId);
      // 非 null は enabled で担保する（未解決のうちは問い合わせない）
      const params = { params: { path: { tenant_id: tenantUuid!, user_id: userUuid } } };
      return {
        queryKey: ['get', USER_PROFILE_PATH, params],
        queryFn: async ({ signal }: { signal: AbortSignal }) => {
          const { data, error, response } = await fetchClient.GET(USER_PROFILE_PATH, {
            ...params,
            signal,
          });
          if (response.status === 404) return null;
          if (error) throw Object.assign(new Error(error.message), { status: response.status });
          return data;
        },
        enabled: !!tenantUuid && !!userUuid,
        retry: false,
      };
    }),
  );
}

export function useMeQuery(options?: { enabled?: MaybeRefOrGetter<boolean> }) {
  return apiClient.useQuery('get', '/v1/auth/me', undefined, {
    staleTime: AUTH_ME_STALE_TIME_MS,
    retry: meRetry,
    retryDelay: meRetryDelay,
    ...(options?.enabled !== undefined ? { enabled: options.enabled } : {}),
  });
}

/** 有効な OAuth ログインプロバイダー一覧（backend 起動時に固定されるため長めにキャッシュ）。 */
export function useOAuthProvidersQuery() {
  return apiClient.useQuery('get', '/v1/auth/oauth/providers', undefined, {
    staleTime: Infinity,
    retry: false,
  });
}

export const personalTokensQueryOptions = () =>
  apiClient.queryOptions('get', '/v1/personal_tokens', undefined, { retry: false });

export function usePersonalTokensQuery() {
  return apiClient.useQuery('get', '/v1/personal_tokens', undefined, { retry: false });
}

export function useCreatePersonalTokenMutation() {
  return apiClient.useMutation('post', '/v1/personal_tokens');
}

export function useRevokePersonalTokenMutation() {
  return apiClient.useMutation('delete', '/v1/personal_tokens/{id}');
}

export const devicesQueryOptions = () =>
  apiClient.queryOptions('get', '/v1/users/me/devices', undefined, { retry: false });

export function useDevicesQuery() {
  return apiClient.useQuery('get', '/v1/users/me/devices', undefined, { retry: false });
}

export function useRevokeDeviceMutation() {
  return apiClient.useMutation('delete', '/v1/users/me/devices/{id}');
}

export function useCreateDesktopAuthCodeMutation() {
  return apiClient.useMutation('post', '/v1/desktop/auth/codes');
}

export function useTenantsQuery() {
  return apiClient.useQuery('get', '/v1/tenants', undefined, {
    staleTime: AUTH_ME_STALE_TIME_MS,
    retry: false,
  });
}

export const REVIEWS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/reviews' as const;
export const REVIEW_FINDINGS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/review-findings' as const;
export const REVIEWED_PRS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/reviews/pull-requests' as const;
export const REVIEW_SUMMARY_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/reviews/summary' as const;

/** レビューのある PR の一覧（集計つき）。 */
export function useReviewedPullRequestsQuery(tenantId: string, projectId: string) {
  return apiClient.useQuery(
    'get',
    REVIEWED_PRS_PATH,
    { params: { path: { tenant_id: tenantId, project_id: projectId } } },
    { retry: false },
  );
}

export function useReviewRoundsQuery(
  tenantId: string,
  projectId: string,
  pr: MaybeRefOrGetter<number | null>,
) {
  return useQuery(
    computed(() => {
      const prNumber = toValue(pr);
      return {
        ...apiClient.queryOptions(
          'get',
          REVIEWS_PATH,
          {
            params: {
              path: { tenant_id: tenantId, project_id: projectId },
              query: { pr: prNumber ?? 0 },
            },
          },
          { retry: false },
        ),
        enabled: prNumber !== null,
      };
    }),
  );
}

export function useReviewFindingsQuery(
  tenantId: string,
  projectId: string,
  pr: MaybeRefOrGetter<number | null>,
) {
  return useQuery(
    computed(() => {
      const prNumber = toValue(pr);
      return {
        ...apiClient.queryOptions(
          'get',
          REVIEW_FINDINGS_PATH,
          {
            params: {
              path: { tenant_id: tenantId, project_id: projectId },
              query: { pr: prNumber ?? 0 },
            },
          },
          { retry: false },
        ),
        enabled: prNumber !== null,
      };
    }),
  );
}

export function useReviewSummaryQuery(
  tenantId: string,
  projectId: string,
  pr: MaybeRefOrGetter<number | null>,
) {
  return useQuery(
    computed(() => {
      const prNumber = toValue(pr);
      return {
        ...apiClient.queryOptions(
          'get',
          REVIEW_SUMMARY_PATH,
          {
            params: {
              path: { tenant_id: tenantId, project_id: projectId },
              query: { pr: prNumber ?? 0 },
            },
          },
          { retry: false },
        ),
        enabled: prNumber !== null,
      };
    }),
  );
}

export function useCreateReviewMutation() {
  return apiClient.useMutation('post', REVIEWS_PATH);
}

export function useUpdateFindingStateMutation() {
  return apiClient.useMutation(
    'patch',
    '/v1/tenants/{tenant_id}/projects/{project_id}/review-findings/{id}',
  );
}

export function useLoginMutation() {
  return apiClient.useMutation('post', '/v1/auth/login');
}

export function useTotpSetupMutation() {
  return apiClient.useMutation('post', '/v1/auth/2fa/totp/setup');
}

export function useTotpVerifySetupMutation() {
  return apiClient.useMutation('post', '/v1/auth/2fa/totp/verify-setup');
}

export function useVerifyTwoFactorMutation() {
  return apiClient.useMutation('post', '/v1/auth/2fa/verify');
}

export function useRegisterMutation() {
  return apiClient.useMutation('post', '/v1/auth/register');
}

export function useUpdateProfileMutation() {
  return apiClient.useMutation('patch', '/v1/auth/me');
}

export function useLogoutMutation() {
  return apiClient.useMutation('post', '/v1/auth/logout');
}

export function useVerifyEmailMutation() {
  return apiClient.useMutation('post', '/v1/auth/verify-email');
}

export function useResendVerificationEmailMutation() {
  return apiClient.useMutation('post', '/v1/auth/resend-verification-email');
}

export function usePasswordResetRequestMutation() {
  return apiClient.useMutation('post', '/v1/auth/password-reset/request');
}

export function usePasswordResetVerifyMutation() {
  return apiClient.useMutation('post', '/v1/auth/password-reset/verify');
}

export function usePasswordResetCompleteMutation() {
  return apiClient.useMutation('post', '/v1/auth/password-reset/complete');
}

/** 連携済み OAuth プロバイダー。設定画面の認証方法セクションで一覧と解除に使う。 */
export const oauthConnectionsQueryOptions = () =>
  apiClient.queryOptions('get', '/v1/auth/oauth/connections', undefined, { retry: false });

export function useOAuthConnectionsQuery() {
  return apiClient.useQuery('get', '/v1/auth/oauth/connections', undefined, { retry: false });
}

/**
 * 登録済みパスキー。認証方法セクションでは件数だけ使う。
 * backend は「パスワード・OAuth 連携・パスキー」を合わせて最後の1つを守るので、
 * 解除できるかの見立てをサーバーと揃えるために数える。
 */
export function usePasskeysQuery() {
  return apiClient.useQuery('get', '/v1/auth/passkeys', undefined, { retry: false });
}

export function useDisconnectOAuthMutation() {
  return apiClient.useMutation('delete', '/v1/auth/oauth/connections/{provider}');
}

/** OAuth だけで登録した利用者の初回パスワード設定。 */
export function useSetPasswordMutation() {
  return apiClient.useMutation('post', '/v1/auth/password');
}

export function usePasswordChangeMutation() {
  return apiClient.useMutation('post', '/v1/auth/password/change');
}

export const DRIVE_FOLDERS_PATH = '/v1/tenants/{tenant_id}/drive/folders' as const;
export const DRIVE_FILES_PATH = '/v1/tenants/{tenant_id}/drive/files' as const;
export const DRIVE_FILE_PATH = '/v1/tenants/{tenant_id}/drive/files/{id}' as const;
export const DRIVE_FILE_CONTENT_UPDATE_PATH =
  '/v1/tenants/{tenant_id}/drive/files/{id}/content' as const;
export const DRIVE_FILE_TASKS_PATH = '/v1/tenants/{tenant_id}/drive/files/{id}/tasks' as const;
export const DRIVE_FILE_CONTENT_PATH = '/v1/drive/files/{id}/content' as const;

/** ドキュメント一覧の取得件数（API の上限）。 */
export const DOCUMENT_LIST_LIMIT = 200;

/** プロジェクトのルートフォルダ（ドキュメント置き場）。呼び出し側は先頭 1 件を使う。 */
export const projectRootFoldersQueryOptions = (
  tenantId: TenantUuid | null | undefined,
  projectId: ProjectUuid | null | undefined,
) =>
  apiClient.queryOptions('get', DRIVE_FOLDERS_PATH, {
    params: {
      path: { tenant_id: tenantId ?? '' },
      query: { project_id: projectId ?? '', root_only: true },
    },
  });

/** ルートフォルダ直下の `.md`（名前順）。`q` はファイル名の部分一致。 */
export const projectDocumentsQueryOptions = (
  tenantId: TenantUuid | null | undefined,
  folderId: string | null | undefined,
  q: string,
) =>
  apiClient.queryOptions('get', DRIVE_FILES_PATH, {
    params: {
      path: { tenant_id: tenantId ?? '' },
      query: {
        folder_id: folderId ?? '',
        mime_type: 'text/markdown',
        sort: 'name_asc',
        limit: DOCUMENT_LIST_LIMIT,
        ...(q ? { q } : {}),
      },
    },
  });

export const driveFileQueryOptions = (tenantId: TenantUuid | null | undefined, fileId: string) =>
  apiClient.queryOptions('get', DRIVE_FILE_PATH, {
    params: { path: { tenant_id: tenantId ?? '', id: fileId } },
  });

export const driveFileTasksQueryOptions = (
  tenantId: TenantUuid | null | undefined,
  fileId: string,
) =>
  apiClient.queryOptions('get', DRIVE_FILE_TASKS_PATH, {
    params: { path: { tenant_id: tenantId ?? '', id: fileId } },
  });

/**
 * ファイル本文（テキスト）。配信は `Last-Modified` 付きで `Cache-Control` が無く、
 * ブラウザが経験則で古い本文を返しうるので、毎回 `ETag` で確かめさせる（`no-cache`）。
 */
export type DriveFileContent = {
  body: string;
  /** 本文と同じ応答の `ETag`。`If-Match` はこれに結び付ける（メタデータの ETag は別取得で食い違いうる） */
  etag: string | null;
};

export const driveFileContentQueryOptions = (fileId: string) => {
  const init = { params: { path: { id: fileId } } };
  return {
    queryKey: ['get', DRIVE_FILE_CONTENT_PATH, init] as const,
    queryFn: async ({ signal }: { signal: AbortSignal }): Promise<DriveFileContent> => {
      const { data, error, response } = await fetchClient.GET(DRIVE_FILE_CONTENT_PATH, {
        ...init,
        parseAs: 'text',
        cache: 'no-cache',
        signal,
      });
      if (error !== undefined) throw error;
      return { body: data, etag: response.headers.get('ETag') };
    },
  };
};

export function createTestApiClient(fetchImpl: (input: Request) => Promise<Response>) {
  return createClient<paths>(
    createFetchClient<paths>({
      baseUrl: 'http://test.local/api',
      fetch: fetchImpl,
    }),
  );
}

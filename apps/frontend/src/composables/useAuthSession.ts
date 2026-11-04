import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { computed, watch, type MaybeRefOrGetter, toValue } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';
import {
  meQueryOptions,
  queryErrorStatus,
  sessionExpired,
  useLogoutMutation,
  useMeQuery,
} from '@/lib/api-vue-query';
import { currentLocationPath, signInUrlForCurrentPage } from '@/composables/useRedirectAfter';
import { twoFactorPath } from '@/lib/two-factor';
import { authUserSchema, useAuthStore } from '@/stores/auth';

export function useAuthSession(options?: { guard?: MaybeRefOrGetter<boolean> }) {
  const authStore = useAuthStore();
  const pageContext = usePageContext();
  const queryClient = useQueryClient();
  const meQueryEnabled = computed(() =>
    options?.guard === undefined ? true : toValue(options.guard),
  );
  const meQuery = useMeQuery({ enabled: meQueryEnabled });
  const logoutMutation = useLogoutMutation();

  watch(
    () => meQuery.data.value,
    (user) => {
      if (!user) return;
      const result = authUserSchema(user);
      if (result instanceof type.errors) {
        authStore.clearUser();
        return;
      }
      authStore.setUser(result);
    },
    { immediate: true },
  );

  function redirectToSignInIfNeeded() {
    if (!toValue(options?.guard)) return;
    const pathname = pageContext.urlPathname;
    if (!['/signin', '/signup'].includes(pathname)) {
      // 半認証（2FA 待ち）のセッションは /auth/me が 403 になる。サインインへ戻さず 2FA 画面へ進める。
      // どちらも今のページを戻り先として持ち越す
      window.location.assign(
        queryErrorStatus(meQuery.error?.value) === 403
          ? twoFactorPath(currentLocationPath())
          : signInUrlForCurrentPage(),
      );
    }
  }

  const isSettledError = computed(() => meQuery.isError.value && !meQuery.isFetching.value);
  // 401（セッション無し）と 403（半認証。2FA 画面へ進める）だけがサインアウト扱い。
  // 5xx・通信断はセッションを保ち、再試行を出す
  const isUnauthorized = computed(() => {
    const status = queryErrorStatus(meQuery.error?.value);
    return isSettledError.value && (status === 401 || status === 403);
  });
  const isTransientError = computed(() => isSettledError.value && !isUnauthorized.value);

  watch(
    isUnauthorized,
    (unauthorized) => {
      if (!unauthorized) return;
      authStore.clearUser();
      redirectToSignInIfNeeded();
    },
    { immediate: true },
  );

  async function logout() {
    try {
      await logoutMutation.mutateAsync({} as never);
    } finally {
      authStore.clearUser();
      await queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey });
      window.location.assign('/signin');
    }
  }

  function retryMe() {
    void meQuery.refetch?.();
  }

  return { meQuery, logout, logoutMutation, isTransientError, retryMe, sessionExpired };
}

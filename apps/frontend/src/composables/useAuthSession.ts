import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { computed, watch, type MaybeRefOrGetter, toValue } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';
import {
  isSessionRejected,
  meQueryOptions,
  sessionExpired,
  useLogoutMutation,
  useMeQuery,
} from '@/lib/api-vue-query';
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
      window.location.assign('/signin');
    }
  }

  const isSettledError = computed(() => meQuery.isError.value && !meQuery.isFetching.value);
  // セッションの拒否（401 / 403）だけがサインアウト。5xx・通信断はセッションを保ち、再試行を出す
  const isUnauthorized = computed(
    () => isSettledError.value && isSessionRejected(meQuery.error?.value),
  );
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

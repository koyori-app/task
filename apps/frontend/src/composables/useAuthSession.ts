import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { computed, watch, type MaybeRefOrGetter, toValue } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { meQueryOptions, useLogoutMutation, useMeQuery } from '@/lib/api-vue-query';
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
      // 半認証（2FA 待ち）のセッションは /auth/me が 403 になる。サインインへ戻さず 2FA 画面へ進める
      const status = (meQuery.error?.value as { response?: { status?: number } } | null)?.response
        ?.status;
      window.location.assign(
        status === 403
          ? twoFactorPath(`${window.location.pathname}${window.location.search}`)
          : '/signin',
      );
    }
  }

  const isSettledError = computed(() => meQuery.isError.value && !meQuery.isFetching.value);

  watch(
    isSettledError,
    (settled) => {
      if (!settled) return;
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

  return { meQuery, logout, logoutMutation };
}

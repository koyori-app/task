import { useQueryClient } from '@tanstack/vue-query';
import { computed, watch, type MaybeRefOrGetter, toValue } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { meQueryOptions, useLogoutMutation, useMeQuery } from '@/lib/api-vue-query';
import { useAuthStore, type AuthUser } from '@/stores/auth';

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
      if (user) {
        authStore.setUser(user as AuthUser);
      }
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

  watch(
    () => meQuery.isError.value,
    (isError) => {
      if (!isError) return;
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

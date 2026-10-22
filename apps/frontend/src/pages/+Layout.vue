<!-- https://vike.dev/Layout -->

<script setup lang="ts">
import AppSidebarSkeleton from '@/components/sidebar/AppSidebarSkeleton.vue';
import AppBreadcrumbs from '@/components/AppBreadcrumbs.vue';
import { useBreadcrumbs } from '@/composables/useBreadcrumbs';
import { Button, Drawer } from '@koyori-app/ui-vue';
import { PhSidebarSimple } from '@phosphor-icons/vue';
import { defaultDocument, useEventListener, useMediaQuery } from '@vueuse/core';
import EmailNotVerified from '@/components/auth/EmailNotVerified.vue';
import { useAuthSession } from '@/composables/useAuthSession';
import { ClientOnly } from 'vike-vue/ClientOnly';
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, defineAsyncComponent, ref } from 'vue';

const TanStackDevtools = import.meta.env.DEV
  ? defineAsyncComponent(() => import('@/components/devtools/TanStackDevtoolsClient.vue'))
  : null;
const isDev = import.meta.env.DEV;

const pageContext = usePageContext();
const breadcrumbs = useBreadcrumbs(() => pageContext);
// サインイン前に開くページ。認証ガードを外さないと /signin へ飛ばされる
const isAuthPage = computed(() =>
  [
    '/signin',
    '/signup',
    '/auth/reset-password',
    '/verify-email',
    '/desktop/authorize',
    '/invitations/accept',
  ].includes(pageContext.urlPathname),
);

const { meQuery, logout } = useAuthSession({
  guard: computed(() => !isAuthPage.value),
});

const isEmailVerified = computed(() => meQuery.data.value?.email_verified ?? true);

// ---- サイドバーの開閉 ----
// 広い画面は常設で、開閉ボタンはアイコンだけの表示（rail）を切り替える。
// 狭い画面はモーダルの Drawer で、開閉ボタンは Drawer を開け閉めする。
const isMobile = useMediaQuery('(max-width: 768px)');
// rail は cookie に残す。名前と値は旧サイドバーのものを引き継ぐ（sidebar_state=false が畳んだ状態 = rail）
const SIDEBAR_COOKIE = 'sidebar_state';
const rail = ref(defaultDocument?.cookie.includes(`${SIDEBAR_COOKIE}=false`) ?? false);
// 狭い画面の Drawer はページを覆うので、開いた状態は残さない
const drawerOpen = ref(false);
const sidebarRail = computed(() => !isMobile.value && rail.value);

function setRail(value: boolean) {
  rail.value = value;
  document.cookie = `${SIDEBAR_COOKIE}=${!value}; path=/; max-age=${60 * 60 * 24 * 7}`;
}

function toggleSidebar() {
  if (isMobile.value) drawerOpen.value = !drawerOpen.value;
  else setRail(!rail.value);
}

useEventListener('keydown', (event: KeyboardEvent) => {
  if (event.key === 'b' && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    toggleSidebar();
  }
});

const AppSidebar = defineAsyncComponent(() => import('@/components/sidebar/AppSidebar.vue'));
const AppHeader = defineAsyncComponent(() => import('@/components/header/AppHeader.vue'));
</script>

<template>
  <ClientOnly v-if="isDev && TanStackDevtools">
    <component :is="TanStackDevtools" />
  </ClientOnly>
  <slot v-if="isAuthPage" />
  <div
    v-else-if="meQuery.isPending.value"
    class="flex min-h-svh items-center justify-center text-muted-foreground text-sm"
  >
    読み込み中…
  </div>
  <EmailNotVerified
    v-else-if="meQuery.isSuccess.value && !isEmailVerified"
    :email="meQuery.data.value!.email"
    @logout="logout"
  />
  <!--
    テナントとアカウントは全幅のヘッダーに置く（サイドバーの上下ではなく）。
    1 段目をヘッダー、2 段目をサイドバー + 本文にし、スクロールは本文だけにする。
  -->
  <div v-else-if="meQuery.isSuccess.value" class="flex h-svh flex-col overflow-hidden">
    <AppHeader />
    <div class="flex min-h-0 flex-1">
      <Drawer
        label="ナビゲーション"
        placement="left"
        :modal="isMobile"
        :open="drawerOpen"
        @close="drawerOpen = false"
      >
        <Suspense>
          <AppSidebar :rail="sidebarRail" @update:rail="setRail" @close="drawerOpen = false" />
          <template #fallback>
            <AppSidebarSkeleton :rail="sidebarRail" />
          </template>
        </Suspense>
      </Drawer>
      <main class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
        <header class="flex h-16 shrink-0 items-center gap-2">
          <div class="flex w-full min-w-0 items-center gap-2 px-4">
            <Button
              variant="ghost"
              ariaLabel="サイドバーを開閉"
              :ariaExpanded="isMobile ? drawerOpen : !rail"
              ariaControls="app-sidebar"
              @click="toggleSidebar"
            >
              <template #icon><PhSidebarSimple /></template>
            </Button>
            <!-- koyori の縦線は左右に余白を持ち高さいっぱいに伸びるため、右だけ空けて高さ 1rem で中央に置く -->
            <hr aria-orientation="vertical" class="koyori-separator ml-0 mr-2 h-4 self-center" />
            <AppBreadcrumbs v-bind="breadcrumbs" />
          </div>
        </header>
        <div class="flex flex-1 flex-col gap-4 p-4 pt-0">
          <slot />
        </div>
      </main>
    </div>
  </div>
</template>

<!-- https://vike.dev/Layout -->

<script setup lang="ts">
import AppSidebarSkeleton from '@/components/sidebar/AppSidebarSkeleton.vue';
import AppBreadcrumbs from '@/components/AppBreadcrumbs.vue';
import { useBreadcrumbs } from '@/composables/useBreadcrumbs';
import AppTenantRail from '@/components/tenant-rail/AppTenantRail.vue';
import { tenantStatusLabel } from '@/lib/tenant-status-label';
import { useTenantStore } from '@/stores/tenant';
import { Alert, Button, Drawer, Toaster } from '@koyori-app/ui-vue';
import { PhSidebarSimple } from '@phosphor-icons/vue';
import { defaultDocument, useEventListener, useMediaQuery } from '@vueuse/core';
import EmailNotVerified from '@/components/auth/EmailNotVerified.vue';
import { useAuthSession } from '@/composables/useAuthSession';
import { AUTH_PAGE_PATHS } from '@/lib/auth-pages';
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
const isAuthPage = computed(() => AUTH_PAGE_PATHS.includes(pageContext.urlPathname));

const { meQuery, logout, isTransientError, retryMe, sessionExpired } = useAuthSession({
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

// 狭い画面の本文の上の帯に出す今のテナント。文言はサイドバーの見出しと揃える
const tenantStore = useTenantStore();
const currentTenantLabel = computed(() =>
  tenantStatusLabel({
    loading: tenantStore.isLoading,
    error: tenantStore.error,
    tenants: tenantStore.tenants,
    selected: tenantStore.selectedTenant,
  }),
);

const AppSidebar = defineAsyncComponent(() => import('@/components/sidebar/AppSidebar.vue'));
</script>

<template>
  <ClientOnly v-if="isDev && TanStackDevtools">
    <component :is="TanStackDevtools" />
  </ClientOnly>
  <!-- 一時的な失敗ではサインイン画面へ飛ばさず、今のセッションを保って再試行を出す -->
  <div
    v-if="!isAuthPage && (sessionExpired || isTransientError)"
    class="fixed inset-x-0 top-0 z-50 p-2"
  >
    <Alert v-if="sessionExpired" message="セッションが切れました。サインイン画面へ移動します。" />
    <Alert v-else message="サーバーに接続できませんでした。" :onRetry="retryMe" />
  </div>
  <slot v-if="isAuthPage" />
  <div
    v-else-if="meQuery.isPending.value"
    class="flex min-h-svh items-center justify-center text-muted-foreground text-sm"
  >
    読み込み中…
  </div>
  <EmailNotVerified
    v-else-if="meQuery.data.value && !isEmailVerified"
    :email="meQuery.data.value!.email"
    @logout="logout"
  />
  <!--
    ヘッダーは置かない。左からテナントのレール、サイドバー、本文を横に並べ、スクロールは本文だけにする。
    狭い画面ではレールとサイドバーを Drawer に入れて横に並べる（幅 320px）。
  -->
  <div v-else-if="meQuery.data.value" class="flex h-svh overflow-hidden">
    <div class="flex shrink-0 [--koyori-drawer-width:20rem]">
      <Drawer
        label="ナビゲーション"
        placement="left"
        :modal="isMobile"
        :open="drawerOpen"
        @close="drawerOpen = false"
      >
        <!-- modal の dialog は縦並びなので、横並びはこの入れ物が持つ。
             Sidebar 自身の枠と角丸は消し、境目の線は包む要素が 1 本だけ引く -->
        <div
          class="flex h-full min-h-0 [--koyori-sidebar-border:0] [--koyori-sidebar-radius:0]"
          data-slot="app-navigation"
        >
          <AppTenantRail @close="drawerOpen = false" />
          <div
            class="flex min-h-0"
            :class="isMobile ? 'min-w-0 flex-1 border-l' : 'shrink-0 border-x'"
          >
            <Suspense>
              <AppSidebar :rail="sidebarRail" @update:rail="setRail" @close="drawerOpen = false" />
              <template #fallback>
                <AppSidebarSkeleton :rail="sidebarRail" />
              </template>
            </Suspense>
          </div>
        </div>
      </Drawer>
    </div>
    <main class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
      <header class="flex shrink-0 items-center gap-2" :class="isMobile ? 'h-12' : 'h-16'">
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
          <!-- 狭い画面はパンくずの代わりに今のテナントを出す。押すと Drawer を開く -->
          <button
            v-if="isMobile"
            type="button"
            class="flex min-w-0 items-center gap-2 rounded-md px-1 py-1 font-medium text-sm hover:bg-accent"
            aria-haspopup="dialog"
            aria-controls="app-sidebar"
            data-slot="current-tenant"
            @click="drawerOpen = true"
          >
            <!-- TenantIcon（ui-vue 0.15.0 の IconTile の公開後に 24px で置く） -->
            <span class="truncate">{{ currentTenantLabel }}</span>
          </button>
          <template v-else>
            <!-- koyori の縦線は左右に余白を持ち高さいっぱいに伸びるため、右だけ空けて高さ 1rem で中央に置く -->
            <hr aria-orientation="vertical" class="koyori-separator ml-0 mr-2 h-4 self-center" />
            <AppBreadcrumbs v-bind="breadcrumbs" />
          </template>
        </div>
      </header>
      <div class="flex flex-1 flex-col gap-4 p-4 pt-0">
        <slot />
      </div>
    </main>
  </div>
  <Toaster />
</template>

<script setup lang="ts">
import { Sidebar } from '@koyori-app/ui-vue';
import { useRouteAlignedTenantId } from '@/composables/useRouteAlignedTenantId';
import { useTenantStore } from '@/stores/tenant';
import { useProjectsQuery } from '@/lib/api-vue-query';
import { usePageContext } from 'vike-vue/usePageContext';
import { navigate } from 'vike/client/router';
import { computed, watch } from 'vue';

import { Calendar, House, ListTodo } from '@lucide/vue';
import NavMain from '@/components/sidebar/NavMain.vue';
import NavProjects from '@/components/sidebar/NavProjects.vue';
import { shouldCloseSidebarOnNavigate } from '@/components/sidebar/sidebar-navigation';

defineProps<{
  /** アイコンだけの表示（広い画面の畳んだ状態）。状態は呼び出し側が持つ */
  rail?: boolean;
}>();

const emit = defineEmits<{
  'update:rail': [rail: boolean];
  /** ナビから遷移した。狭い画面ではサイドバー（Drawer）を閉じる */
  close: [];
}>();

const pageContext = usePageContext();
const tenantStore = useTenantStore();

const tenantSlug = computed(() => {
  const { tenant } = pageContext.routeParams;
  return typeof tenant === 'string' ? tenant : '';
});

const myTasksUrl = computed(() => (tenantSlug.value ? `/${tenantSlug.value}/my-tasks` : '#'));

const routeAlignedTenantId = useRouteAlignedTenantId(
  computed(() => tenantStore.tenants),
  tenantSlug,
);

const projectsQuery = useProjectsQuery(routeAlignedTenantId);

const navProjects = computed(() => projectsQuery.data.value ?? []);

const navProjectsLoading = computed(
  () =>
    projectsQuery.isLoading.value ||
    (Boolean(tenantSlug.value) && routeAlignedTenantId.value === null && tenantStore.isLoading),
);

watch(tenantSlug, (slug) => void tenantStore.loadTenants(slug || undefined), { immediate: true });

function retryProjects() {
  void projectsQuery.refetch();
}

// ---- プロジェクト作成導線（編集・削除は各プロジェクトの設定ページへ集約） ----
function onCreateProject() {
  // 作成ボタンはリンクではないので、本文のイベント委譲では閉じられない
  emit('close');
  void navigate(`/${tenantSlug.value}/projects/new`);
}

const data = computed(() => ({
  navMain: [
    {
      title: 'ホーム',
      url: tenantSlug.value ? `/${tenantSlug.value}` : '/',
      icon: House,
      isActive: pageContext.urlPathname === `/${tenantSlug.value}`,
    },
    {
      title: 'My Tasks',
      url: myTasksUrl.value,
      icon: ListTodo,
      isActive: pageContext.urlPathname === `/${pageContext.routeParams.tenant}/my-tasks`,
    },
    {
      title: 'カレンダー',
      url: tenantSlug.value ? `/${tenantSlug.value}/calendar` : '#',
      icon: Calendar,
      isActive: pageContext.urlPathname === `/${pageContext.routeParams.tenant}/calendar`,
    },
  ],
}));

/**
 * ナビから遷移したら狭い画面のサイドバーを閉じる（判定は sidebar-navigation に切り出し）。
 * Sidebar の根の要素に付け、本文のリンクの click を委譲で拾う。
 */
function closeOnNavigate(event: MouseEvent) {
  if (shouldCloseSidebarOnNavigate(event)) emit('close');
}
</script>

<template>
  <Sidebar
    id="app-sidebar"
    label="ナビゲーション"
    :rail="rail"
    @rail-change="emit('update:rail', $event)"
    @click="closeOnNavigate"
  >
    <!-- テナント外のページ（/settings/... など）ではテナント文脈が無く、
           リンク先も一覧も作れないためテナント依存のナビ自体を出さない。 -->
    <NavMain v-if="tenantSlug" :items="data.navMain" />
    <NavProjects
      v-if="tenantSlug"
      :tenant-slug="tenantSlug"
      :projects="navProjects"
      :current-path="pageContext.urlPathname"
      :loading="navProjectsLoading"
      :error="projectsQuery.isError.value"
      @retry="retryProjects"
      @create="onCreateProject"
    />
  </Sidebar>
</template>

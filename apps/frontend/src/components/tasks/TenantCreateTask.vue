<script setup lang="ts">
import { computed, ref } from 'vue';
import { useQueryClient } from '@tanstack/vue-query';
import { usePageContext } from 'vike-vue/usePageContext';
import DashboardCreateTask from '@/components/dashboard/DashboardCreateTask.vue';
import { useHydrated } from '@/composables/useHydrated';
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import type { components } from '@/generated/api';
import { useMeQuery, useProjectsQuery } from '@/lib/api-vue-query';

/**
 * テナント配下のどのページからも T でプロジェクトを選んでタスクを作る入口。
 * 画面が自前の T を持つとき（ホーム・タスク一覧・ドキュメント詳細）はそちらが優先される。
 * 選べるのはサイドバーと同じ共有プロジェクトだけ。作ったあとはその場に留まる
 */
const MY_TASKS_PATH = '/v1/tenants/{tenant_id}/users/me/tasks' as const;
const SCHEDULE_PATH = '/v1/tenants/{tenant_id}/schedule' as const;

const pageContext = usePageContext();
const { tenantId } = useResolvedTenantId(
  computed(() => String(pageContext.routeParams.tenant ?? '')),
);
const projectsQuery = useProjectsQuery(tenantId);
const projects = computed<components['schemas']['ProjectResponse'][]>(
  () => projectsQuery.data.value ?? [],
);
const me = useMeQuery();
const hydrated = useHydrated();
const queryClient = useQueryClient();

const open = ref(false);
const canCreate = computed(() => hydrated.value && !!tenantId.value && projects.value.length > 0);
// プロジェクト配下のページでは、そのプロジェクトを選んだ状態で開く
const initialProjectId = computed(() => {
  const key = pageContext.routeParams.projectKey;
  return key ? projects.value.find((p) => p.key === key)?.id : undefined;
});

useKeyboardShortcuts(
  {
    t: () => {
      if (canCreate.value) open.value = true;
    },
  },
  { fallback: true },
);

// タスク一覧と検索は CreateTaskDialog が更新する。ここではマイタスクとカレンダーを足す
function onCreated() {
  void Promise.all([
    queryClient.invalidateQueries({ queryKey: ['get', MY_TASKS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', SCHEDULE_PATH] }),
  ]);
}
</script>

<template>
  <DashboardCreateTask
    v-if="canCreate && tenantId"
    :key="tenantId"
    v-model:open="open"
    :tenant-id="tenantId"
    :projects="projects"
    :current-user-id="me.data.value?.id"
    :initial-project-id="initialProjectId"
    @created="onCreated"
  />
</template>

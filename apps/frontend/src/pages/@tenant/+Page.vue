<script setup lang="ts">
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import HomeDashboard from '@/components/dashboard/HomeDashboard.vue';
import { Button } from '@koyori-app/ui-vue';
import { parseHomeDashboardUrlState } from '@/lib/home-dashboard-url-state';

const pageContext = usePageContext();
const slug = computed(() => String(pageContext.routeParams.tenant ?? ''));
const { tenantId, resolvedTenant, isError, isTenantNotFound, tenantsQuery } =
  useResolvedTenantId(slug);
// 共有プロジェクトを作れるのはオーナーとテナント Admin（require_tenant_admin と同じ）。
const canCreateProject = computed(() => {
  const tenant = resolvedTenant.value;
  return !!tenant && (tenant.membership === 'Owner' || tenant.member_role === 'Admin');
});
// タスクの絞り込みとページは URL に持ち、タスクを開いて戻っても同じ画面に戻す
const initialUrlState = computed(() => parseHomeDashboardUrlState(pageContext.urlParsed?.search));
</script>

<template>
  <div v-if="isError" role="alert" class="p-6 text-sm">
    ワークスペースを取得できませんでした。
    <Button variant="tertiary" label="再試行" @click="tenantsQuery.refetch()" />
  </div>
  <div v-else-if="isTenantNotFound" class="p-6 text-sm text-muted-foreground">
    ワークスペースが見つかりません。<a href="/" class="underline">ホームへ戻る</a>
  </div>
  <HomeDashboard
    v-else-if="tenantId"
    :key="tenantId"
    :tenant-id="tenantId"
    :tenant-slug="slug"
    :can-create-project="canCreateProject"
    :initial-url-state="initialUrlState"
  />
  <p v-else role="status" class="p-6 text-sm text-muted-foreground">読み込み中…</p>
</template>

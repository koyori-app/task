<script setup lang="ts">
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import HomeDashboard from '@/components/dashboard/HomeDashboard.vue';
import { Button } from '@koyori-app/ui-vue';

const pageContext = usePageContext();
const slug = computed(() => String(pageContext.routeParams.tenant ?? ''));
const { tenantId, isError, isTenantNotFound, tenantsQuery } = useResolvedTenantId(slug);
</script>

<template>
  <div v-if="isError" role="alert" class="p-6 text-sm">
    ワークスペースを取得できませんでした。
    <Button variant="tertiary" label="再試行" @click="tenantsQuery.refetch()" />
  </div>
  <div v-else-if="isTenantNotFound" class="p-6 text-sm text-muted-foreground">
    ワークスペースが見つかりません。<a href="/" class="underline">ホームへ戻る</a>
  </div>
  <HomeDashboard v-else-if="tenantId" :key="tenantId" :tenant-id="tenantId" :tenant-slug="slug" />
  <p v-else role="status" class="p-6 text-sm text-muted-foreground">読み込み中…</p>
</template>

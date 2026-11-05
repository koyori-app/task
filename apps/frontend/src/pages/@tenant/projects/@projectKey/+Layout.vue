<script setup lang="ts">
import { useProjectEvents } from '@/composables/useProjectEvents';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { usePageContext } from 'vike-vue/usePageContext';
import { computed } from 'vue';

// プロジェクト配下の画面で接続を 1 本に保つため、ページではなくレイアウトで購読する
const pageContext = usePageContext();
const { tenantId } = useResolvedTenantId(
  computed(() => String(pageContext.routeParams.tenant ?? '')),
);
const { projectId } = useResolvedProjectId(
  tenantId,
  computed(() => String(pageContext.routeParams.projectKey ?? '')),
);
useProjectEvents(tenantId, projectId);
</script>

<template>
  <slot />
</template>

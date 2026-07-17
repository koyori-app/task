<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import ProjectCreateForm from '@/components/projects/ProjectCreateForm.vue';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);
</script>

<template>
  <div class="flex flex-col gap-6 px-4 pb-10 pt-2">
    <div v-if="isTenantResolving" class="flex justify-center py-16">
      <Loader2 class="h-8 w-8 animate-spin text-muted-foreground" />
    </div>

    <p v-else-if="isTenantResolveError" class="py-16 text-center text-sm text-destructive">
      ページの読み込みに失敗しました
    </p>

    <p v-else-if="isTenantNotFound" class="py-16 text-center text-sm text-muted-foreground">
      テナントが見つかりません
    </p>

    <ProjectCreateForm v-else-if="tenantId" :tenant-id="tenantId" :tenant-slug="tenantDisplayId" />
  </div>
</template>

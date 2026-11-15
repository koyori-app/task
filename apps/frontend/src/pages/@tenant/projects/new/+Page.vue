<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import ProjectCreateForm from '@/components/projects/ProjectCreateForm.vue';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { isTenantAdmin } from '@/lib/tenant-permissions';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const {
  tenantId,
  resolvedTenant,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);

// 共有プロジェクトを作れるのはテナントのオーナーと Admin だけ（作成 API が他は 403 にする）
const canCreate = computed(() => isTenantAdmin(resolvedTenant.value));
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

    <p v-else-if="tenantId && !canCreate" class="py-16 text-center text-sm text-muted-foreground">
      プロジェクトを作成できるのは、テナントのオーナーと Admin だけです。
    </p>

    <ProjectCreateForm v-else-if="tenantId" :tenant-id="tenantId" :tenant-slug="tenantDisplayId" />
  </div>
</template>

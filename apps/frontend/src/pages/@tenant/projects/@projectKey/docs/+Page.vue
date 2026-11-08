<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import DocumentList from '@/components/documents/DocumentList.vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));
const initialQuery = computed(() => pageContext.urlParsed?.search?.q ?? '');

const {
  tenantId,
  isTenantNotFound,
  isResolving: isTenantResolving,
  isError: isTenantResolveError,
} = useResolvedTenantId(tenantDisplayId);
const {
  projectId,
  isProjectNotFound,
  isResolving: isProjectResolving,
  isError: isProjectError,
} = useResolvedProjectId(tenantId, projectKey);
</script>

<template>
  <div v-if="isTenantResolving || isProjectResolving" class="flex justify-center py-16">
    <Loader2 class="size-8 animate-spin text-muted-foreground" />
  </div>
  <p
    v-else-if="isTenantResolveError || isProjectError"
    class="py-16 text-center text-sm text-destructive"
  >
    ページの読み込みに失敗しました
  </p>
  <p
    v-else-if="isTenantNotFound || isProjectNotFound"
    class="py-16 text-center text-sm text-muted-foreground"
  >
    プロジェクトが見つかりません
  </p>
  <!-- プロジェクトを跨ぐクライアント遷移で検索語やクエリの引数を持ち越さない -->
  <DocumentList
    v-else-if="tenantId && projectId"
    :key="projectId"
    :tenant-id="tenantId"
    :tenant-slug="tenantDisplayId"
    :project-id="projectId"
    :project-key="projectKey"
    :initial-query="initialQuery"
  />
</template>

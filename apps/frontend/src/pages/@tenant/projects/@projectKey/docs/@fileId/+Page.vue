<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import { useData } from 'vike-vue/useData';
import { usePageContext } from 'vike-vue/usePageContext';

import DocumentDetail from '@/components/documents/DocumentDetail.vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import type { Data } from './+data';

const pageContext = usePageContext();
// Storybook 等で data が無いときは undefined（本文はクライアントの再描画へ倒れる）
const data = useData<Data>() as Data | undefined;
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));
const fileId = computed(() => String(pageContext.routeParams.fileId ?? ''));
const initialEditing = computed(() => pageContext.urlParsed?.search?.edit === '1');

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
  <!-- 別のドキュメントへのクライアント遷移で編集状態や下書きを持ち越さない -->
  <DocumentDetail
    v-else-if="tenantId && projectId"
    :key="fileId"
    :tenant-id="tenantId"
    :tenant-slug="tenantDisplayId"
    :project-id="projectId"
    :project-key="projectKey"
    :file-id="fileId"
    :initial-editing="initialEditing"
    :initial-html="data?.descriptionHtml ?? null"
    :initial-source="data?.descriptionSource ?? null"
  />
</template>

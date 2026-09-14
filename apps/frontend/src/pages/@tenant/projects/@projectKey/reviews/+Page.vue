<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import ReviewFindingsView from '@/components/reviews/ReviewFindingsView.vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { useMeQuery } from '@/lib/api-vue-query';
import { parseReviewFindingsUrlState } from '@/lib/review-findings-url-state';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));

type UrlParsedContext = {
  urlParsed?: {
    search?: Record<string, string>;
    pathname?: string;
    searchOriginal?: string | null;
  };
};

/** SSR でも client navigation でも、最初に描く状態は URL だけから決める。 */
const initialUrl = computed(() =>
  parseReviewFindingsUrlState((pageContext as UrlParsedContext | undefined)?.urlParsed?.search),
);

/**
 * 現在の URL（pathname + search）。SSR の findingHref が path を自前で組まずに
 * 済むよう渡す——自前で組むと、この画面が持たぬ query が SSR 出力の链から落ちる。
 */
const requestUrl = computed(() => {
  const parsed = (pageContext as UrlParsedContext | undefined)?.urlParsed;
  if (!parsed?.pathname) return null;
  return `${parsed.pathname}${parsed.searchOriginal ?? ''}`;
});

const {
  tenantId,
  tenantOwnerId,
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

const meQuery = useMeQuery();

const isLoading = computed(
  () => isTenantResolving.value || isProjectResolving.value || meQuery.isPending.value,
);
const isError = computed(
  () => isTenantResolveError.value || isProjectError.value || meQuery.isError.value,
);
const isNotFound = computed(() => isTenantNotFound.value || isProjectNotFound.value);
</script>

<template>
  <div class="flex flex-col gap-6 px-4 pt-2 pb-10">
    <div v-if="isLoading" class="flex justify-center py-16">
      <Loader2 class="text-muted-foreground h-8 w-8 animate-spin" />
    </div>

    <p v-else-if="isError" class="text-destructive py-16 text-center text-sm">
      ページの読み込みに失敗しました
    </p>

    <p v-else-if="isNotFound" class="text-muted-foreground py-16 text-center text-sm">
      プロジェクトが見つかりません
    </p>

    <!--
      `:key` でプロジェクトごとに作り直す。

      vike-vue はクライアント遷移で同じ `+Page.vue` に解決される URL 間では
      コンポーネントを差し替えず patch するため、これが無いとクエリの引数
      （setup 時の値で固定される）も、選択中の PR や絞り込みも前のプロジェクトの
      ままになる。設定画面（`settings/+Page.vue`）と同じ扱い。
    -->
    <ReviewFindingsView
      v-else-if="tenantId && projectId && meQuery.data.value"
      :key="projectId"
      :tenant-id="tenantId"
      :tenant-slug="tenantDisplayId"
      :project-id="projectId"
      :project-key="projectKey"
      :viewer-id="meQuery.data.value.id"
      :tenant-owner-id="tenantOwnerId"
      :initial-url-state="initialUrl.state"
      :initial-url-warnings="initialUrl.warnings"
      :request-url="requestUrl"
    />
  </div>
</template>

<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import ReviewPullRequestDetail from '@/components/reviews/ReviewPullRequestDetail.vue';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { useMeQuery } from '@/lib/api-vue-query';
import { parsePrNumber, parseReviewDetailUrlState, reviewListHref } from '@/lib/review-url-state';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));
const prNumber = computed(() => parsePrNumber(pageContext.routeParams.prNumber));

type UrlParsedContext = {
  urlParsed?: {
    search?: Record<string, string>;
    pathname?: string;
    searchOriginal?: string | null;
  };
};

/** SSR でも client navigation でも、最初に描く状態は URL だけから決める。 */
const initialUrl = computed(() =>
  parseReviewDetailUrlState((pageContext as UrlParsedContext | undefined)?.urlParsed?.search),
);

/**
 * 現在の URL（pathname + search）。SSR で指摘へのリンクを組むときに path を自前で
 * 組まずに済むよう渡す（自前で組むと、この画面が持たない query が SSR の出力から落ちる）。
 */
const requestUrl = computed(() => {
  const parsed = (pageContext as UrlParsedContext | undefined)?.urlParsed;
  if (!parsed?.pathname) return null;
  return `${parsed.pathname}${parsed.searchOriginal ?? ''}`;
});

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

const meQuery = useMeQuery();

const isLoading = computed(
  () => isTenantResolving.value || isProjectResolving.value || meQuery.isPending.value,
);
const isError = computed(
  () =>
    isTenantResolveError.value ||
    isProjectError.value ||
    (meQuery.isError.value && !meQuery.data.value),
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

    <div
      v-else-if="prNumber === null"
      class="text-muted-foreground flex flex-col items-center gap-2 py-16 text-center text-sm"
      data-testid="pr-not-found"
    >
      <p>PR が見つかりません</p>
      <a
        class="text-link underline underline-offset-2"
        :href="reviewListHref(tenantDisplayId, projectKey)"
      >
        PR の一覧へ
      </a>
    </div>

    <!--
      `:key` でプロジェクトと PR ごとに作り直す。vike-vue はクライアント遷移で同じ
      `+Page.vue` に解決される URL 間ではコンポーネントを差し替えず patch するため、
      これが無いとクエリの引数（setup 時の値で固定される）も、絞り込みも前の PR のままになる。
    -->
    <ReviewPullRequestDetail
      v-else-if="tenantId && projectId && meQuery.data.value"
      :key="`${projectId}:${prNumber}`"
      :tenant-id="tenantId"
      :tenant-slug="tenantDisplayId"
      :project-id="projectId"
      :project-key="projectKey"
      :pr-number="prNumber"
      :viewer-id="meQuery.data.value.id"
      :initial-url-state="initialUrl.state"
      :initial-url-warnings="initialUrl.warnings"
      :request-url="requestUrl"
    />
  </div>
</template>

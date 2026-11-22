<script setup lang="ts">
import { Loader2 } from '@lucide/vue';
import { computed, watch } from 'vue';
import { navigate } from 'vike/client/router';
import { usePageContext } from 'vike-vue/usePageContext';

import ReviewPullRequestList from '@/components/reviews/ReviewPullRequestList.vue';
import { useHydrated } from '@/composables/useHydrated';
import { useResolvedProjectId } from '@/composables/useResolvedProjectId';
import { useResolvedTenantId } from '@/composables/useResolvedTenantId';
import { legacyReviewDetailHref, parseReviewListUrlState } from '@/lib/review-url-state';

const pageContext = usePageContext();
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));

type UrlParsedContext = {
  urlParsed?: { search?: Record<string, string>; pathname?: string };
};
const urlParsed = computed(() => (pageContext as UrlParsedContext | undefined)?.urlParsed);

/** SSR でも client navigation でも、最初に描く状態は URL だけから決める。 */
const initialUrl = computed(() => parseReviewListUrlState(urlParsed.value?.search));

/**
 * 旧 URL（`/reviews?pr=N`）の転送先。投稿済みの GitHub の要約コメントと通知メールの
 * リンクがこの形で残っているので、詳細（`/reviews/N`）へ届かせる。
 */
const legacyHref = computed(() =>
  urlParsed.value?.pathname
    ? legacyReviewDetailHref(urlParsed.value.pathname, urlParsed.value.search)
    : null,
);
/** `pr` があるのに転送できない（番号として読めない）。一覧を出して知らせる */
const legacyWarnings = computed(() => {
  const raw = urlParsed.value?.search?.pr;
  return raw !== undefined && legacyHref.value === null
    ? [`URL の PR 番号「${raw.slice(0, 40)}」は正の整数ではないため無視しました。`]
    : [];
});

// 同じ +Page.vue の中の遷移（一覧を開いたまま旧 URL へ移る）でも転送する
const hydrated = useHydrated();
watch(
  [hydrated, legacyHref],
  ([ready, href]) => {
    if (ready && href) void navigate(href, { overwriteLastHistoryEntry: true });
  },
  { immediate: true },
);

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

// 転送を待つ間は読み込み中の表示だけを出す（SSR も同じ表示になる）
const isLoading = computed(
  () => legacyHref.value !== null || isTenantResolving.value || isProjectResolving.value,
);
const isError = computed(() => isTenantResolveError.value || isProjectError.value);
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
      （setup 時の値で固定される）も、絞り込みも前のプロジェクトのままになる。
      設定画面（`settings/+Page.vue`）と同じ扱い。
    -->
    <ReviewPullRequestList
      v-else-if="tenantId && projectId"
      :key="projectId"
      :tenant-id="tenantId"
      :tenant-slug="tenantDisplayId"
      :project-id="projectId"
      :project-key="projectKey"
      :initial-url-state="initialUrl.state"
      :initial-url-warnings="[...legacyWarnings, ...initialUrl.warnings]"
    />
  </div>
</template>

<script setup lang="ts">
import { PhCaretRight } from '@phosphor-icons/vue';
import { computed, onMounted, ref, watch } from 'vue';
import {
  Alert,
  Avatar,
  Badge,
  Button,
  DataList,
  DataListRow,
  Dropdown,
  Input,
  Pagination,
  Skeleton,
  Switch,
  type BadgeTone,
  type DataListColumn,
  type DropdownItem,
} from '@koyori-app/ui-vue';

import { useOAuthConnectionsQuery, useReviewedPullRequestsQuery } from '@/lib/api-vue-query';
import { pullRequestState } from '@/lib/forge-pull-request';
import type { ReviewedPullRequest } from '@/lib/review-findings';
import {
  DEFAULT_PR_STATE_FILTER,
  PR_PAGE_SIZE,
  PR_REVIEWED_FILTERS,
  PR_SORTS,
  PR_STATE_FILTERS,
  PR_UNRESOLVED_FILTERS,
  filterPullRequests,
  hasGithubConnection,
  initialPrStateFilter,
  isMyPullRequest,
  noMatchMessage,
  pageCount,
  prStateFilterLabel,
  pullRequestAuthors,
  pullRequestCounts,
  reviewedAgo,
  sortPullRequests,
  viewerGithubLogin,
  type PrListFilters,
  type PrReviewedFilter,
  type PrSort,
  type PrStateFilter,
  type PrUnresolvedFilter,
} from '@/lib/review-pr-list';
import {
  DEFAULT_REVIEW_LIST_URL_STATE,
  PR_QUERY_MAX_LENGTH,
  applyReviewListUrlState,
  reviewDetailHref,
  type ReviewListUrlState,
} from '@/lib/review-url-state';

const props = defineProps<{
  tenantId: string;
  tenantSlug: string;
  projectId: string;
  projectKey: string;
  /** SSR が URL から復元した初期表示。 */
  initialUrlState?: ReviewListUrlState;
  /** URL の不正値を黙って捨てず、利用者へ知らせる。 */
  initialUrlWarnings?: string[];
}>();

const initialUrlState = props.initialUrlState ?? DEFAULT_REVIEW_LIST_URL_STATE;
const query = ref(initialUrlState.q);
// 利用者が選んだ「PR の状態」（URL の `pr_state`）。null は選んでいないで、URL に載せない
const prStateExplicit = ref<PrStateFilter | null>(initialUrlState.prState);
const prStateFilter = ref<PrStateFilter>(prStateExplicit.value ?? DEFAULT_PR_STATE_FILTER);
// 一覧が初めて届いたときに既定の「PR の状態」を 1 回だけ確かめるための印。利用者が先に
// 触ったら（URL に `pr_state` があれば）確かめない
const prStateDecided = ref(prStateExplicit.value !== null);
const authorFilter = ref<string | null>(initialUrlState.author);
const unresolvedFilter = ref<PrUnresolvedFilter>(initialUrlState.unresolved);
const reviewedFilter = ref<PrReviewedFilter>(initialUrlState.reviewed);
const mineOnly = ref(initialUrlState.mine);
const sort = ref<PrSort>(initialUrlState.sort);
/** null は 1 ページ目（URL に載せない） */
const page = ref<number | null>(initialUrlState.page);
const urlWarnings = ref([...(props.initialUrlWarnings ?? [])]);
/** 経過の表示と期間の絞り込みの基準。マウント時に 1 回だけ取る */
const now = ref<Date | null>(null);

// API は PR 単位で全件を集計して返すので、絞り込み・並べ替え・ページ分割は画面側で行う
// （サーバーで切っても集計の仕事は減らない）。
// ponytail: 全件を一度に受ける。PR が数千件になって一覧の転送が重くなったら API にページングを足す
const prsQuery = useReviewedPullRequestsQuery(props.tenantId, props.projectId);
const pullRequests = computed<ReviewedPullRequest[]>(() => prsQuery.data.value ?? []);

// --- 自分の PR ---
//
// PR の作成者は GitHub のユーザー名なので、連携が無いと見分けられない。
// 一覧は連携一覧を待たずに出す（連携一覧は外部への問い合わせで遅れることがある）。

const connectionsQuery = useOAuthConnectionsQuery();
/** 連携一覧が取れなければ未連携と同じ扱い（スイッチを無効にする）。 */
const githubLogin = computed(() =>
  viewerGithubLogin(connectionsQuery.data.value?.connections ?? []),
);
/** 連携はあるのにユーザー名が分からない（再連携で直る）。 */
const githubNeedsRelink = computed(
  () =>
    githubLogin.value === null &&
    hasGithubConnection(connectionsQuery.data.value?.connections ?? []),
);
const connectionsFailed = computed(() => connectionsQuery.isError.value);
/**
 * 実際に作成者で絞っているか。連携一覧の待機中・取得失敗の間は `mineOnly`（URL の mine=1）が
 * 立っていても絞れていないので、文言とボタンはこちらで出し分ける。
 */
const mineApplied = computed(() => mineOnly.value && githubLogin.value !== null);
const myPullRequestCount = computed(
  () => pullRequests.value.filter((pr) => isMyPullRequest(pr, githubLogin.value)).length,
);
/** 「自分の PR」が使えない理由。使えるときは null */
const mineUnavailableReason = computed(() => {
  if (githubLogin.value) return null;
  if (connectionsQuery.isPending.value) return '連携状態を確認しています…';
  if (connectionsFailed.value) {
    return '連携状態を取得できなかったため使えません。時間をおいて開き直してください。';
  }
  if (githubNeedsRelink.value) {
    return 'GitHub のユーザー名を確認できませんでした。アカウント設定で GitHub を連携し直すと使えます。';
  }
  return 'GitHub と連携すると使えます。PR の作成者を GitHub のユーザー名で判定するためです。';
});

// --- 絞り込み・並べ替え・ページ ---

const filters = computed<PrListFilters>(() => ({
  q: query.value,
  state: prStateFilter.value,
  author: authorFilter.value,
  mineLogin: mineOnly.value ? githubLogin.value : null,
  unresolved: unresolvedFilter.value,
  reviewed: reviewedFilter.value,
}));
const filtered = computed(() =>
  filterPullRequests(pullRequests.value, filters.value, now.value ?? new Date()),
);
const sorted = computed(() => sortPullRequests(filtered.value, sort.value));
const pageTotal = computed(() => pageCount(filtered.value.length));
const currentPage = computed(() => Math.min(Math.max(1, page.value ?? 1), pageTotal.value));
const pageItems = computed(() => {
  const start = (currentPage.value - 1) * PR_PAGE_SIZE;
  return sorted.value.slice(start, start + PR_PAGE_SIZE);
});
const hasConditions = computed(
  () =>
    authorFilter.value !== null ||
    unresolvedFilter.value !== 'all' ||
    reviewedFilter.value !== 'all',
);

function currentUrlState(): ReviewListUrlState {
  return {
    q: query.value,
    prState: prStateExplicit.value,
    author: authorFilter.value,
    unresolved: unresolvedFilter.value,
    reviewed: reviewedFilter.value,
    mine: mineOnly.value,
    sort: sort.value,
    page: page.value,
  };
}

/** 詳細の「PR の一覧」が絞り込みを保って戻れるよう、いまのクエリを控える */
function rememberSearch() {
  try {
    const own = applyReviewListUrlState(new URL('http://list.local/'), currentUrlState());
    window.sessionStorage.setItem(`reviews:list:${props.projectId}`, own.search);
  } catch {
    // sessionStorage が使えなければ、詳細はクエリ無しの一覧へ戻る
  }
}

function writeUrl() {
  if (import.meta.env.SSR) return;
  const next = applyReviewListUrlState(new URL(window.location.href), currentUrlState());
  window.history.replaceState(window.history.state, '', next);
  rememberSearch();
}

/**
 * 利用者の操作で URL を書き直したときに呼ぶ。書き直した時点で不正値は URL から
 * 消えているので、出しっぱなしだった警告も片付ける。絞り込みを変えたら 1 ページ目へ戻す。
 */
function commit(resetPage = true) {
  if (resetPage) page.value = null;
  urlWarnings.value = [];
  writeUrl();
}

/**
 * URL から来た `mine` と `page` は、一覧と連携の状態が揃ってから確かめる
 * （未連携の `mine=1` と範囲外の `page` は、無視・最後のページに寄せて理由を出す）。
 */
const urlCheckReady = computed(
  () =>
    prsQuery.isSuccess.value &&
    // 既定の「PR の状態」を確かめた後の一覧でページを数える（暫定の Open で数えると、
    // 「すべて」に倒れる一覧のページを誤って補正する）。空の一覧は確かめる対象が無い
    (prStateDecided.value || pullRequests.value.length === 0) &&
    !connectionsQuery.isPending.value,
);
let urlCheckPending = true;
function checkUrlState() {
  if (!urlCheckPending || !urlCheckReady.value) return;
  urlCheckPending = false;
  let changed = false;
  if (mineOnly.value && githubLogin.value === null && connectionsFailed.value) {
    // 連携していないとは限らない（一時的な失敗）。共有された mine=1 は URL に残し、
    // 開き直せば効くようにする
    urlWarnings.value.push(
      '連携状態を取得できなかったため、URL の「自分の PR だけ」を適用できませんでした。',
    );
  } else if (mineOnly.value && githubLogin.value === null) {
    urlWarnings.value.push(
      githubNeedsRelink.value
        ? 'GitHub のユーザー名を確認できないため、URL の「自分の PR だけ」は無視しました。'
        : 'GitHub と連携していないため、URL の「自分の PR だけ」は無視しました。',
    );
    mineOnly.value = false;
    changed = true;
  }
  if (page.value !== null && page.value > pageTotal.value) {
    urlWarnings.value.push(
      `URL のページ ${page.value} はありません（全 ${pageTotal.value} ページ）。最後のページを表示しています。`,
    );
    page.value = pageTotal.value;
    changed = true;
  }
  if (changed) writeUrl();
}
watch(urlCheckReady, checkUrlState, { immediate: true });

/**
 * 一覧が初めて届いたら、既定の「PR の状態」を確かめる（Open が無いか、URL のページが
 * Open の一覧には無く「すべて」なら在るときは「すべて」）。
 */
watch(
  pullRequests,
  (list) => {
    if (list.length === 0 || prStateDecided.value) return;
    prStateDecided.value = true;
    prStateFilter.value = initialPrStateFilter(list, page.value);
  },
  // 同じ QueryClient での再マウントでは一覧がキャッシュで最初から揃っていて、変化として届かない
  { immediate: true },
);

// vike は同じ +Page.vue に解決される URL 間の遷移で component を差し替えず patch する。
// setup で読んだ initialUrlState はそのままでは凍るため、props の更新もここへ流す。
watch(
  () => [props.initialUrlState, props.initialUrlWarnings] as const,
  ([state, warnings]) => {
    const next = state ?? DEFAULT_REVIEW_LIST_URL_STATE;
    const nextWarnings = warnings ?? [];
    if (
      JSON.stringify(next) === JSON.stringify(currentUrlState()) &&
      JSON.stringify(nextWarnings) === JSON.stringify(urlWarnings.value)
    ) {
      return;
    }
    query.value = next.q;
    authorFilter.value = next.author;
    unresolvedFilter.value = next.unresolved;
    reviewedFilter.value = next.reviewed;
    mineOnly.value = next.mine;
    sort.value = next.sort;
    page.value = next.page;
    // ページの確かめ（checkUrlState）は状態の絞り込みで数えるので、先に決める
    prStateExplicit.value = next.prState;
    if (next.prState !== null) {
      prStateFilter.value = next.prState;
      prStateDecided.value = true;
    } else if (pullRequests.value.length > 0) {
      prStateFilter.value = initialPrStateFilter(pullRequests.value, next.page);
      prStateDecided.value = true;
    } else {
      prStateDecided.value = false;
    }
    urlWarnings.value = [...nextWarnings];
    urlCheckPending = true;
    checkUrlState();
    // 補正が無いと writeUrl を通らないので、ここでも控える（空のクエリも控えて、前の条件を消す）
    rememberSearch();
  },
);

onMounted(() => {
  now.value = new Date();
  rememberSearch();
});

// --- 絞り込みの部品 ---

function radio<T extends string>(values: readonly T[], label: (value: T) => string, current: T) {
  return values.map((value): DropdownItem => ({
    value,
    label: label(value),
    kind: 'radio',
    checked: value === current,
  }));
}

const UNRESOLVED_LABELS: Record<PrUnresolvedFilter, string> = {
  all: 'すべて',
  yes: 'あり',
  no: 'なし',
};
const REVIEWED_LABELS: Record<PrReviewedFilter, string> = {
  all: 'すべて',
  '7d': '7 日以内',
  '30d': '30 日以内',
  '90d': '90 日以内',
};
const SORT_LABELS: Record<PrSort, string> = {
  recent: '最終レビューが新しい順',
  unresolved: '未解決が多い順',
  number: '番号が大きい順',
};
// GitHub のユーザー名に使えない文字なので、作成者の名前と重ならない
const ALL_AUTHORS = '*';

const stateItems = computed(() => radio(PR_STATE_FILTERS, prStateFilterLabel, prStateFilter.value));
const authorItems = computed<DropdownItem[]>(() => [
  { value: ALL_AUTHORS, label: 'すべて', kind: 'radio', checked: authorFilter.value === null },
  ...pullRequestAuthors(pullRequests.value).map((author): DropdownItem => ({
    value: author,
    label: author,
    kind: 'radio',
    checked: authorFilter.value?.toLowerCase() === author.toLowerCase(),
  })),
]);
const unresolvedItems = computed(() =>
  radio(PR_UNRESOLVED_FILTERS, (v) => UNRESOLVED_LABELS[v], unresolvedFilter.value),
);
const reviewedItems = computed(() =>
  radio(PR_REVIEWED_FILTERS, (v) => REVIEWED_LABELS[v], reviewedFilter.value),
);
const sortItems = computed(() => radio(PR_SORTS, (v) => SORT_LABELS[v], sort.value));

function onQueryInput(value: string) {
  query.value = value;
  commit();
}
function setPrState(value: string) {
  prStateDecided.value = true;
  prStateFilter.value = value as PrStateFilter;
  prStateExplicit.value = value as PrStateFilter;
  commit();
}
function setAuthor(value: string) {
  authorFilter.value = value === ALL_AUTHORS ? null : value;
  commit();
}
function setUnresolved(value: string) {
  unresolvedFilter.value = value as PrUnresolvedFilter;
  commit();
}
function setReviewed(value: string) {
  reviewedFilter.value = value as PrReviewedFilter;
  commit();
}
function setSort(value: string) {
  sort.value = value as PrSort;
  commit();
}
function setMineOnly(value: boolean) {
  mineOnly.value = value;
  commit();
}
function clearConditions() {
  authorFilter.value = null;
  unresolvedFilter.value = 'all';
  reviewedFilter.value = 'all';
  commit();
}
function goToPage(next: number) {
  const clamped = Math.min(Math.max(1, next), pageTotal.value);
  page.value = clamped === 1 ? null : clamped;
  commit(false);
}

// --- 表 ---

const columns: DataListColumn[] = [
  { id: 'pr', label: 'PR' },
  { id: 'author', label: '作成者', width: '9rem' },
  { id: 'high-medium', label: 'High・Medium', width: '8.5rem' },
  { id: 'low-nit', label: 'Low・Nit', width: '5.5rem' },
  { id: 'awaiting', label: '承認待ち', width: '5.5rem' },
  { id: 'round', label: 'Round', width: '4.5rem' },
  { id: 'reviewed', label: '最終レビュー', width: '7.5rem' },
  { id: 'open', label: '開く', width: '3.5rem' },
];

function countCells(pr: ReviewedPullRequest): { id: string; count: number; tone: BadgeTone }[] {
  const counts = pullRequestCounts(pr);
  return [
    { id: 'high-medium', count: counts.highMedium, tone: 'danger' },
    { id: 'low-nit', count: counts.lowNit, tone: 'neutral' },
    { id: 'awaiting', count: counts.awaiting, tone: 'warning' },
  ];
}

function absoluteTime(iso: string): string {
  return new Date(iso).toLocaleString('ja-JP');
}

function detailHref(pr: ReviewedPullRequest): string {
  return reviewDetailHref(props.tenantSlug, props.projectKey, pr.pr_number);
}
</script>

<template>
  <div class="flex w-full flex-col gap-4">
    <h1 class="text-2xl font-bold tracking-tight">レビュー</h1>

    <div
      v-if="urlWarnings.length > 0"
      role="status"
      class="bg-muted text-muted-foreground rounded-md border p-3 text-sm"
      data-testid="url-warning"
    >
      <p v-for="warning in urlWarnings" :key="warning">{{ warning }}</p>
    </div>

    <div v-if="prsQuery.isPending.value" class="flex flex-col gap-2" data-testid="pr-list-loading">
      <Skeleton v-for="n in 3" :key="n" height="2.75rem" />
    </div>

    <Alert
      v-else-if="prsQuery.isError.value"
      prefix="エラー"
      message="レビューを読み込めませんでした"
      retry-label="再試行"
      :on-retry="() => void prsQuery.refetch()"
    />

    <div
      v-else-if="pullRequests.length === 0"
      class="bg-muted flex flex-col items-center gap-1 rounded-lg px-4 py-8 text-center"
      data-testid="pr-list-empty"
    >
      <p class="text-sm font-semibold">レビューはまだありません</p>
      <p class="text-muted-foreground text-xs">PR にレビューを出すと、ここに並びます。</p>
    </div>

    <template v-else>
      <div class="flex flex-col gap-2">
        <div class="flex flex-wrap items-center gap-2" data-testid="pr-filters">
          <div class="w-full sm:w-64">
            <Input
              type="search"
              aria-label="番号・タイトルで探す"
              autocomplete="off"
              placeholder="番号・タイトルで探す"
              :max-length="PR_QUERY_MAX_LENGTH"
              :value="query"
              :on-value-change="onQueryInput"
            />
          </div>
          <Dropdown
            variant="secondary"
            :label="`状態: ${prStateFilterLabel(prStateFilter)}`"
            :items="stateItems"
            :on-select="setPrState"
          />
          <Dropdown
            variant="secondary"
            :label="`作成者: ${authorFilter ?? 'すべて'}`"
            :items="authorItems"
            :on-select="setAuthor"
          />
          <Dropdown
            variant="secondary"
            :label="`未解決: ${UNRESOLVED_LABELS[unresolvedFilter]}`"
            :items="unresolvedItems"
            :on-select="setUnresolved"
          />
          <Dropdown
            variant="secondary"
            :label="`最終レビュー: ${REVIEWED_LABELS[reviewedFilter]}`"
            :items="reviewedItems"
            :on-select="setReviewed"
          />
          <Switch
            label="自分の PR"
            :checked="mineApplied"
            :disabled="githubLogin === null"
            aria-described-by="mine-only-help"
            :on-checked-change="setMineOnly"
            data-testid="mine-only"
          />
          <Dropdown
            variant="ghost"
            :label="`並び: ${SORT_LABELS[sort]}`"
            :items="sortItems"
            :on-select="setSort"
          />
        </div>
        <p
          id="mine-only-help"
          :class="mineUnavailableReason ? 'text-muted-foreground text-xs' : 'sr-only'"
          data-testid="mine-only-help"
        >
          <template v-if="mineUnavailableReason">{{ mineUnavailableReason }}</template>
          <template v-else>GitHub @{{ githubLogin }} で判定 · {{ myPullRequestCount }} 件</template>
        </p>
      </div>

      <div
        v-if="filtered.length === 0"
        class="bg-muted flex flex-col items-center gap-3 rounded-lg px-4 py-8 text-center"
        data-testid="no-pr-match"
      >
        <p class="text-sm font-semibold">
          {{ noMatchMessage({ ...filters, mineLogin: mineApplied ? githubLogin : null }) }}
        </p>
        <div
          v-if="mineApplied || prStateFilter !== 'all' || hasConditions"
          class="flex flex-wrap justify-center gap-2"
        >
          <Button
            v-if="mineApplied"
            type="button"
            variant="tertiary"
            label="すべての PR から探す"
            @click="setMineOnly(false)"
          />
          <Button
            v-if="prStateFilter !== 'all'"
            type="button"
            variant="tertiary"
            label="すべての状態を表示"
            @click="setPrState('all')"
          />
          <Button
            v-if="hasConditions"
            type="button"
            variant="tertiary"
            label="条件を外す"
            @click="clearConditions"
          />
        </div>
      </div>

      <template v-else>
        <DataList
          id="review-pr-list"
          label="レビューのある PR"
          class="[--koyori-data-list-min-width:56rem]"
          :heading-level="2"
          :columns="columns"
          :count="filtered.length"
        >
          <DataListRow v-for="pr in pageItems" :key="pr.pr_number" data-testid="pr-row">
            <td>
              <div class="flex min-w-0 items-start gap-2">
                <component
                  :is="pullRequestState(pr.state).icon"
                  class="mt-0.5 size-4 shrink-0"
                  :class="pullRequestState(pr.state).class"
                  aria-hidden="true"
                />
                <div class="flex min-w-0 flex-col">
                  <span class="flex min-w-0 items-baseline gap-1.5">
                    <span class="text-muted-foreground shrink-0 text-xs">#{{ pr.pr_number }}</span>
                    <a
                      class="min-w-0 truncate font-semibold underline-offset-4 hover:underline"
                      :href="detailHref(pr)"
                      data-testid="pr-title"
                    >
                      {{ pr.pr_title ?? `PR #${pr.pr_number}` }}
                    </a>
                  </span>
                  <span class="text-muted-foreground truncate text-xs" data-testid="pr-state">
                    {{ pullRequestState(pr.state).label
                    }}<template v-if="pr.repository"> · {{ pr.repository }}</template>
                  </span>
                </div>
              </div>
            </td>
            <td>
              <span v-if="pr.pr_author" class="flex min-w-0 items-center gap-1.5">
                <Avatar :name="pr.pr_author" :size="20" />
                <span class="truncate">{{ pr.pr_author }}</span>
              </span>
              <span v-else class="text-muted-foreground">—</span>
            </td>
            <td v-for="cell in countCells(pr)" :key="cell.id" :data-testid="`pr-${cell.id}`">
              <Badge
                v-if="cell.count > 0"
                :label="cell.count"
                :tone="cell.tone"
                appearance="soft"
                size="sm"
              />
              <span v-else class="text-muted-foreground">0</span>
            </td>
            <td>
              <template v-if="pr.rounds > 0">R{{ pr.rounds }}</template>
              <span v-else class="text-muted-foreground">—</span>
            </td>
            <td>
              <time
                v-if="pr.last_reviewed_at"
                class="text-muted-foreground"
                :datetime="pr.last_reviewed_at"
                :title="now ? absoluteTime(pr.last_reviewed_at) : undefined"
              >
                {{ now ? reviewedAgo(pr.last_reviewed_at, now) : '' }}
              </time>
              <span v-else class="text-muted-foreground">—</span>
            </td>
            <td>
              <a
                class="hover:bg-accent inline-flex size-8 items-center justify-center rounded-md"
                :href="detailHref(pr)"
                :aria-label="`#${pr.pr_number} を開く`"
              >
                <PhCaretRight class="size-4" aria-hidden="true" />
              </a>
            </td>
          </DataListRow>
        </DataList>

        <Pagination
          label="PR のページ"
          :page="currentPage"
          :page-size="PR_PAGE_SIZE"
          :total="filtered.length"
          :on-page-change="goToPage"
        />
      </template>
    </template>
  </div>
</template>

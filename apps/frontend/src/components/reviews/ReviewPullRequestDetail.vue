<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { PhArrowSquareOut, PhCaretLeft, PhFile, PhPlus } from '@phosphor-icons/vue';
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { navigate } from 'vike/client/router';
import {
  Alert,
  Button,
  ConfirmDialog,
  Dropdown,
  Input,
  Skeleton,
  Tree,
  type DropdownItem,
} from '@koyori-app/ui-vue';

import LoadErrorState from '@/components/LoadErrorState.vue';
import ReviewFindingRow from '@/components/reviews/ReviewFindingRow.vue';
import ReviewMergeGate from '@/components/reviews/ReviewMergeGate.vue';
import ReviewRoundComposer from '@/components/reviews/ReviewRoundComposer.vue';
import {
  REVIEWED_PRS_PATH,
  REVIEWS_PATH,
  REVIEW_FINDINGS_PATH,
  REVIEW_SUMMARY_PATH,
  useReviewFindingsQuery,
  useReviewRoundsQuery,
  useReviewSummaryQuery,
  useReviewedPullRequestsQuery,
  useUpdateFindingStateMutation,
} from '@/lib/api-vue-query';
import { commitHref, forgeHost, pullRequestHref, type ForgeTarget } from '@/lib/forge-host';
import { pullRequestState } from '@/lib/forge-pull-request';
import { buildFileTree, fileOfTreeNode, treeNodeIdOfFile } from '@/lib/review-file-tree';
import {
  SEVERITIES,
  SEVERITY_LABELS,
  STATES,
  STATE_LABELS,
  UNRESOLVED_STATES,
  filterFindings,
  filterLabel,
  groupFindingsByFile,
  sortFindings,
  transitionErrorMessage,
  type FindingSeverity,
  type FindingState,
  type Review,
  type ReviewFinding,
} from '@/lib/review-findings';
import { reviewedAgo } from '@/lib/review-pr-list';
import {
  DEFAULT_REVIEW_DETAIL_URL_STATE,
  PR_QUERY_MAX_LENGTH,
  applyReviewDetailUrlState,
  relativeReviewDetailFindingHref,
  reviewListHref,
  type ReviewDetailUrlState,
} from '@/lib/review-url-state';

const props = defineProps<{
  tenantId: string;
  tenantSlug: string;
  projectId: string;
  projectKey: string;
  prNumber: number;
  /** 現在の利用者。自分の修正を自分で確認できない旨の説明を出すのに使う */
  viewerId: string;
  /** SSR が URL から復元した初期表示。 */
  initialUrlState?: ReviewDetailUrlState;
  /** URL の不正値を黙って捨てず、利用者へ知らせる。 */
  initialUrlWarnings?: string[];
  /**
   * 現在の URL（pathname + search）。SSR で指摘へのリンクを組む土台に使い、
   * この画面が持たない query（tab= の類）を SSR の出力でも保つ。
   */
  requestUrl?: string | null;
}>();

const queryClient = useQueryClient();

const initialUrlState = props.initialUrlState ?? DEFAULT_REVIEW_DETAIL_URL_STATE;
const query = ref(initialUrlState.q);
const stateFilter = ref<FindingState[]>([...initialUrlState.states]);
const severityFilter = ref<FindingSeverity[]>([...initialUrlState.severities]);
const roundFilter = ref<number | null>(initialUrlState.round);
const reviewerFilter = ref<string | null>(initialUrlState.reviewer);
const selectedFindingId = ref<string | null>(initialUrlState.finding);
const urlWarnings = ref([...(props.initialUrlWarnings ?? [])]);
const isComposerOpen = ref(false);
/** 起票欄に、捨てると失われる下書きがあるか */
const composerDirty = ref(false);
const transitionError = ref<{ id: string; message: string } | null>(null);

const prsQuery = useReviewedPullRequestsQuery(props.tenantId, props.projectId);
const roundsQuery = useReviewRoundsQuery(props.tenantId, props.projectId, () => props.prNumber);
const findingsQuery = useReviewFindingsQuery(props.tenantId, props.projectId, () => props.prNumber);
const summaryQuery = useReviewSummaryQuery(props.tenantId, props.projectId, () => props.prNumber);
const updateState = useUpdateFindingStateMutation();

// --- 見出し ---

/** 一覧の行。レビューが 1 件も無い PR と、一覧を読めなかったときは無い */
const pullRequest = computed(
  () => (prsQuery.data.value ?? []).find((pr) => pr.pr_number === props.prNumber) ?? null,
);
const summary = computed(() => summaryQuery.data.value ?? null);
const rounds = computed<Review[]>(() => roundsQuery.data.value ?? []);

const forgeTarget = computed<ForgeTarget>(() => ({
  host: pullRequest.value?.host ?? null,
  html_url: pullRequest.value?.html_url ?? null,
  repository: pullRequest.value?.repository ?? summary.value?.repository ?? null,
}));
const host = computed(() => forgeHost(forgeTarget.value.host));
// 知らないホストでも、PR 行に URL があれば開ける（ロゴと名前は出せない）
const hostHref = computed(() => pullRequestHref(forgeTarget.value, props.prNumber));

const headingMeta = computed(() =>
  [
    `#${props.prNumber}`,
    pullRequest.value?.pr_author,
    pullRequest.value ? pullRequestState(pullRequest.value.state).label : null,
    forgeTarget.value.repository,
  ]
    .filter(Boolean)
    .join(' · '),
);
const headingTitle = computed(() => pullRequest.value?.pr_title ?? `PR #${props.prNumber}`);

// --- 一覧へ戻る ---

/** 一覧が最後に URL へ書いたクエリ。SSR と最初の描画はクエリ無し（マウント後に読む） */
const listSearch = ref('');
const listHref = computed(
  () => `${reviewListHref(props.tenantSlug, props.projectKey)}${listSearch.value}`,
);
const confirmingLeave = ref(false);

function onBackClick(event: MouseEvent) {
  // 修飾キーつきのクリックは別のタブで開くだけで、下書きを失わない
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  if (!(isComposerOpen.value && composerDirty.value)) return;
  event.preventDefault();
  confirmingLeave.value = true;
}

function confirmLeave() {
  confirmingLeave.value = false;
  composerDirty.value = false;
  void navigate(listHref.value);
}

function onBeforeUnload(event: BeforeUnloadEvent) {
  if (isComposerOpen.value && composerDirty.value) event.preventDefault();
}

// --- URL ---

function currentUrlState(): ReviewDetailUrlState {
  return {
    q: query.value,
    states: stateFilter.value,
    severities: severityFilter.value,
    round: roundFilter.value,
    reviewer: reviewerFilter.value,
    finding: selectedFindingId.value,
  };
}

/** 詳細は URL を置き換えでしか書かない。「戻る」は画面の中の切り替えではなく、画面からの離脱になる */
function writeUrl() {
  if (import.meta.env.SSR) return;
  const next = applyReviewDetailUrlState(new URL(window.location.href), currentUrlState());
  window.history.replaceState(window.history.state, '', next);
}

/**
 * 利用者の操作で URL を書き直したときに呼ぶ。書き直した時点で不正値は URL から
 * 消えているので、出しっぱなしだった警告も片付ける。
 */
function commitUrl() {
  urlWarnings.value = [];
  writeUrl();
}

function applyUrlState(state: ReviewDetailUrlState, warnings: readonly string[]) {
  query.value = state.q;
  stateFilter.value = [...state.states];
  severityFilter.value = [...state.severities];
  roundFilter.value = state.round;
  reviewerFilter.value = state.reviewer;
  selectedFindingId.value = state.finding;
  urlWarnings.value = [...warnings];
  transitionError.value = null;
}

// vike は同じ +Page.vue に解決される URL 間の遷移で component を差し替えず patch する
// （本文の中の、同じ PR の別の指摘へのリンクなど）。setup で読んだ initialUrlState は
// そのままでは凍るため、props の更新もここへ流す。writeUrl は pageContext を動かさない
// ので環はできない。
watch(
  () => [props.initialUrlState, props.initialUrlWarnings] as const,
  ([state, warnings]) => {
    const next = state ?? DEFAULT_REVIEW_DETAIL_URL_STATE;
    const nextWarnings = warnings ?? [];
    if (
      JSON.stringify(next) === JSON.stringify(currentUrlState()) &&
      JSON.stringify(nextWarnings) === JSON.stringify(urlWarnings.value)
    ) {
      return;
    }
    applyUrlState(next, nextWarnings);
  },
);

// --- 絞り込み ---

const allFindings = computed(() => sortFindings(findingsQuery.data.value ?? []));
const reviewerOfRound = computed(
  () => new Map(rounds.value.map((round) => [round.round, round.reviewer.id])),
);
const visibleFindings = computed(() =>
  filterFindings(
    allFindings.value,
    {
      q: query.value,
      states: stateFilter.value,
      severities: severityFilter.value,
      round: roundFilter.value,
      reviewer: reviewerFilter.value,
    },
    reviewerOfRound.value,
  ),
);
const groups = computed(() =>
  groupFindingsByFile(allFindings.value, new Set(visibleFindings.value.map((f) => f.id))),
);
/** 画面に出ている順の指摘（↑ ↓ の移動に使う） */
const orderedFindings = computed(() => groups.value.flatMap((group) => group.findings));

const hasFilters = computed(
  () =>
    query.value.trim() !== '' ||
    stateFilter.value.length > 0 ||
    severityFilter.value.length > 0 ||
    roundFilter.value !== null ||
    reviewerFilter.value !== null,
);

const ALL = 'all';
const stateItems = computed<DropdownItem[]>(() =>
  STATES.map((value) => ({
    value,
    label: STATE_LABELS[value],
    kind: 'checkbox',
    checked: stateFilter.value.includes(value),
  })),
);
const severityItems = computed<DropdownItem[]>(() =>
  SEVERITIES.map((value) => ({
    value,
    label: SEVERITY_LABELS[value],
    kind: 'checkbox',
    checked: severityFilter.value.includes(value),
  })),
);

function roundLabel(round: Review): string {
  return `R${round.round} · ${round.head_sha.slice(0, 7)} · ${round.reviewer.username}${
    round.carried_by ? '（引き継ぎ）' : ''
  }`;
}
const roundItems = computed<DropdownItem[]>(() => [
  { value: ALL, label: 'すべて', kind: 'radio', checked: roundFilter.value === null },
  ...rounds.value.map((round) => ({
    value: String(round.round),
    label: roundLabel(round),
    kind: 'radio' as const,
    checked: roundFilter.value === round.round,
  })),
]);
const reviewers = computed(() => {
  const seen = new Map<string, string>();
  for (const round of rounds.value) {
    if (!seen.has(round.reviewer.id)) seen.set(round.reviewer.id, round.reviewer.username);
  }
  return [...seen].map(([id, username]) => ({ id, username }));
});
const reviewerItems = computed<DropdownItem[]>(() => [
  { value: ALL, label: 'すべて', kind: 'radio', checked: reviewerFilter.value === null },
  ...reviewers.value.map((reviewer) => ({
    value: reviewer.id,
    label: reviewer.username,
    kind: 'radio' as const,
    checked: reviewerFilter.value === reviewer.id,
  })),
]);
const reviewerLabel = computed(() => {
  if (reviewerFilter.value === null) return 'レビュアー: すべて';
  const name = reviewers.value.find((r) => r.id === reviewerFilter.value)?.username;
  return `レビュアー: ${name ?? (roundsQuery.isPending.value ? '読み込み中' : '不明')}`;
});

/**
 * レビュアーの絞り込みは、ラウンドの一覧（どの Round を誰が出したか）が無いと当てられない。
 * 届く前と取得に失敗したときに、空の対応表で全部の指摘を落として「条件に合う指摘は
 * ありません」と出さないよう、待ちと失敗を別に扱う。
 */
const roundsFailed = computed(() => roundsQuery.isError.value && !roundsQuery.data.value);
const reviewerWaiting = computed(
  () => reviewerFilter.value !== null && roundsQuery.isPending.value,
);
const reviewerUnavailable = computed(() => reviewerFilter.value !== null && roundsFailed.value);

/** 選んでいる Round のラウンド（「R3 は a1b2c3d を見ています」の行に使う） */
const selectedRound = computed(
  () => rounds.value.find((round) => round.round === roundFilter.value) ?? null,
);
const selectedRoundCommitHref = computed(() =>
  selectedRound.value ? commitHref(forgeTarget.value, selectedRound.value.head_sha) : null,
);
/** 経過の表示の基準。SSR と最初の描画で食い違わないよう、マウント後に 1 回だけ取る */
const now = ref<Date | null>(null);

function toggled<T>(list: readonly T[], value: T): T[] {
  return list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
}

function onQueryInput(value: string) {
  query.value = value;
  commitUrl();
}
function toggleState(value: string) {
  stateFilter.value = toggled(stateFilter.value, value as FindingState);
  commitUrl();
}
function toggleSeverity(value: string) {
  severityFilter.value = toggled(severityFilter.value, value as FindingSeverity);
  commitUrl();
}
function setRound(value: string) {
  roundFilter.value = value === ALL ? null : Number(value);
  commitUrl();
}
function setReviewer(value: string) {
  reviewerFilter.value = value === ALL ? null : value;
  commitUrl();
}
function clearFilters() {
  query.value = '';
  stateFilter.value = [];
  severityFilter.value = [];
  roundFilter.value = null;
  reviewerFilter.value = null;
  commitUrl();
}
/** マージを塞いでいる指摘だけにする（マージ判定の「未解決を見る」） */
function showUnresolved() {
  query.value = '';
  stateFilter.value = [...UNRESOLVED_STATES];
  severityFilter.value = ['high', 'medium'];
  roundFilter.value = null;
  reviewerFilter.value = null;
  commitUrl();
}

// --- URL の指定が画面に無いとき ---

const roundMissing = computed(
  () =>
    roundsQuery.isSuccess.value &&
    roundFilter.value !== null &&
    !rounds.value.some((round) => round.round === roundFilter.value),
);
const selectedFindingMissing = computed(
  () =>
    findingsQuery.isSuccess.value &&
    selectedFindingId.value !== null &&
    !allFindings.value.some((finding) => finding.id === selectedFindingId.value),
);
const selectedFindingFiltered = computed(
  () =>
    findingsQuery.isSuccess.value &&
    selectedFindingId.value !== null &&
    !selectedFindingMissing.value &&
    // レビュアーの絞り込みを当てられていない間は、見えない理由が絞り込みとは言えない
    !reviewerWaiting.value &&
    !reviewerUnavailable.value &&
    !visibleFindings.value.some((finding) => finding.id === selectedFindingId.value),
);

// --- 選択 ---

function findingHref(id: string): string {
  // SSR は props.requestUrl（+Page が pageContext から渡す現在の URL）を土台にする。
  // path を自前で組むと、この画面が持たない query が SSR の出力から落ちて client と
  // 食い違う。出力は両側とも pathname + search の相対形に揃える。
  const base = import.meta.env.SSR
    ? new URL(props.requestUrl ?? '/', 'http://ssr.local')
    : new URL(window.location.href);
  return relativeReviewDetailFindingHref(base, currentUrlState(), id);
}

/**
 * 選んだ指摘への強制スクロールは、URL から来た選択の一度きり。済んだ id を覚え、
 * 状態の変更 → 取り直しで指摘の data が入れ替わるたびに画面が勝手に戻るのを防ぐ。
 * data も依存に残すのは、共有リンクで直に開いたときは data が届くまで要素が無いため
 * （要素が出るまでは済み扱いにしない）。
 */
const scrolledFindingId = ref<string | null>(null);
watch(
  () => [selectedFindingId.value, findingsQuery.data.value] as const,
  async ([id]) => {
    if (!id) {
      scrolledFindingId.value = null;
      return;
    }
    if (scrolledFindingId.value === id) return;
    await nextTick();
    const el = document.getElementById(`finding-${id}`);
    if (!el) return;
    el.scrollIntoView?.({ block: 'center' });
    scrolledFindingId.value = id;
  },
  { flush: 'post' },
);

/** 画面の操作で選ぶ。行は既に見えているので、スクロールは近いところまでにとどめる */
async function selectFinding(id: string | null, scroll = false) {
  scrolledFindingId.value = id;
  selectedFindingId.value = id;
  if (transitionError.value && transitionError.value.id !== id) transitionError.value = null;
  commitUrl();
  if (!scroll || id === null) return;
  await nextTick();
  document.getElementById(`finding-${id}`)?.scrollIntoView?.({ block: 'nearest' });
}

function toggleFinding(id: string) {
  void selectFinding(selectedFindingId.value === id ? null : id);
}

/** ↑ ↓ を奪わない場所（入力欄・ボタン・メニュー・ファイルの木・ダイアログ） */
const KEY_IGNORED =
  'input, textarea, select, button, [contenteditable], dialog, [role="dialog"], [role="menu"], [role="listbox"], [role="tree"]';

function onKeydown(event: KeyboardEvent) {
  if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey)
    return;
  if (!['ArrowDown', 'ArrowUp', 'Escape'].includes(event.key)) return;
  if (event.target instanceof Element && event.target.closest(KEY_IGNORED)) return;
  // 繰り延べ・取り下げの入力欄を開いている間は、選択を動かさない（書きかけの理由を失う）
  if (document.querySelector('[data-finding-form]')) return;

  if (event.key === 'Escape') {
    if (selectedFindingId.value !== null) void selectFinding(null);
    return;
  }
  const list = orderedFindings.value;
  if (list.length === 0) return;
  const index = list.findIndex((finding) => finding.id === selectedFindingId.value);
  let next: number;
  if (event.key === 'ArrowDown') next = index < 0 ? 0 : Math.min(index + 1, list.length - 1);
  else if (index < 0) return;
  else next = Math.max(index - 1, 0);
  event.preventDefault();
  if (next !== index) void selectFinding(list[next].id, true);
}

onMounted(() => {
  now.value = new Date();
  try {
    const stored = window.sessionStorage.getItem(`reviews:list:${props.projectId}`);
    if (stored?.startsWith('?')) listSearch.value = stored;
  } catch {
    // sessionStorage が使えなければ、クエリ無しの一覧へ戻る
  }
  window.addEventListener('keydown', onKeydown);
  window.addEventListener('beforeunload', onBeforeUnload);
});
onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown);
  window.removeEventListener('beforeunload', onBeforeUnload);
});

// --- ファイルの木 ---

const fileTree = computed(() => buildFileTree(groups.value));
/** 最後に押した木の節。指摘を選んでいないときの選択の表示に使う */
const pressedNodeId = ref<string | null>(null);
const selectedNodeId = computed(() => {
  const selected = orderedFindings.value.find((f) => f.id === selectedFindingId.value);
  return selected ? treeNodeIdOfFile(selected.file || null) : pressedNodeId.value;
});

function groupElementId(index: number): string {
  return `review-file-group-${index}`;
}

/** 木はファイルで絞らず、右の一覧のそのまとまりへ移るだけ */
function onTreeSelect(id: string) {
  const file = fileOfTreeNode(id);
  if (file === undefined) return;
  pressedNodeId.value = id;
  const index = groups.value.findIndex((group) => group.file === file);
  if (index < 0) return;
  document.getElementById(groupElementId(index))?.scrollIntoView?.({ block: 'start' });
}

const PROGRESS_CLASS = {
  danger: 'text-(--koyori-color-danger)',
  warning: 'text-(--koyori-color-warning)',
  success: 'text-(--koyori-color-success)',
} as const;

// --- 状態の遷移と起票 ---

async function invalidateAll() {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['get', REVIEWED_PRS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', REVIEWS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', REVIEW_FINDINGS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', REVIEW_SUMMARY_PATH] }),
  ]);
}

/**
 * 遷移を送ってから取り直しが済むまで。PATCH が返った時点で操作を戻すと、取り直す前の
 * `available_actions` のボタンがもう一度押せてしまう。
 */
const transitioning = ref(false);

async function transition(finding: ReviewFinding, to: FindingState, note: string | null) {
  if (transitioning.value) return;
  transitioning.value = true;
  transitionError.value = null;
  try {
    await updateState.mutateAsync({
      params: {
        path: { tenant_id: props.tenantId, project_id: props.projectId, id: finding.id },
      },
      body: { state: to, note },
    });
  } catch (e) {
    const err = e as { response?: { status?: number }; error?: { message?: string } };
    transitionError.value = {
      id: finding.id,
      message: transitionErrorMessage(err.response?.status, err.error?.message),
    };
  }
  // 失敗した操作の結果が画面に残らないよう、失敗しても正しい状態を取り直す
  try {
    await invalidateAll();
  } finally {
    transitioning.value = false;
  }
}

function openComposer() {
  isComposerOpen.value = true;
}

function closeComposer() {
  isComposerOpen.value = false;
  composerDirty.value = false;
}

async function onRoundCreated() {
  closeComposer();
  await invalidateAll();
}
</script>

<template>
  <div class="flex w-full flex-col gap-4">
    <div>
      <a
        class="text-muted-foreground hover:text-foreground inline-flex items-center gap-1.5 text-sm"
        :href="listHref"
        data-testid="back-to-list"
        @click="onBackClick"
      >
        <PhCaretLeft class="size-3.5" aria-hidden="true" />
        PR の一覧
      </a>
    </div>

    <header class="flex flex-wrap items-end justify-between gap-3">
      <div class="flex min-w-0 flex-col gap-0.5">
        <p class="text-muted-foreground text-xs" data-testid="pr-meta">{{ headingMeta }}</p>
        <h1 class="text-xl font-bold tracking-tight">{{ headingTitle }}</h1>
      </div>
      <div class="flex shrink-0 flex-wrap gap-2">
        <Button
          v-if="hostHref"
          variant="secondary"
          :label="host ? `${host.label} で開く` : 'ホストで開く'"
          :href="hostHref"
          target="_blank"
          rel="noopener noreferrer"
          data-testid="open-on-host"
        >
          <template #icon>
            <component :is="host?.icon ?? PhArrowSquareOut" class="size-4" />
          </template>
        </Button>
        <Button
          v-if="!isComposerOpen"
          type="button"
          label="指摘を起票"
          data-testid="compose"
          @click="openComposer"
        >
          <template #icon><PhPlus class="size-4" /></template>
        </Button>
      </div>
    </header>

    <div
      v-if="urlWarnings.length > 0"
      role="status"
      class="bg-muted text-muted-foreground rounded-md border p-3 text-sm"
      data-testid="url-warning"
    >
      <p v-for="warning in urlWarnings" :key="warning">{{ warning }}</p>
    </div>

    <!-- マージ判定 -->
    <Skeleton v-if="summaryQuery.isPending.value" height="3.25rem" />
    <Alert
      v-else-if="summaryQuery.isError.value || !summary"
      message="マージ判定を読み込めませんでした"
      retry-label="再試行"
      :on-retry="() => void summaryQuery.refetch()"
    />
    <ReviewMergeGate
      v-else
      :summary="summary"
      :target="forgeTarget"
      @show-unresolved="showUnresolved"
      @compose="openComposer"
    />

    <ReviewRoundComposer
      v-if="isComposerOpen"
      :tenant-id="tenantId"
      :project-id="projectId"
      :pr-number="prNumber"
      :next-round="rounds.length + 1"
      @created="onRoundCreated"
      @cancel="closeComposer"
      @dirty-change="(dirty: boolean) => (composerDirty = dirty)"
    />

    <!-- 絞り込み -->
    <div class="flex flex-wrap items-center gap-2" data-testid="finding-filters">
      <div class="w-full sm:w-60">
        <Input
          type="search"
          aria-label="この PR の指摘を探す"
          autocomplete="off"
          placeholder="この PR の指摘を探す"
          :max-length="PR_QUERY_MAX_LENGTH"
          :value="query"
          :on-value-change="onQueryInput"
        />
      </div>
      <Dropdown
        variant="secondary"
        :label="filterLabel('state', stateFilter)"
        :items="stateItems"
        :on-select="toggleState"
      />
      <Dropdown
        variant="secondary"
        :label="filterLabel('severity', severityFilter)"
        :items="severityItems"
        :on-select="toggleSeverity"
      />
      <Dropdown
        variant="secondary"
        :label="roundFilter === null ? 'Round: すべて' : `Round: R${roundFilter}`"
        :items="roundItems"
        :on-select="setRound"
      />
      <Dropdown
        variant="secondary"
        :label="reviewerLabel"
        :items="reviewerItems"
        :on-select="setReviewer"
      />
      <p
        v-if="!reviewerWaiting && !reviewerUnavailable"
        class="text-muted-foreground ml-auto text-xs"
        data-testid="finding-count"
      >
        {{ visibleFindings.length }} 件 · ↑ ↓ で移る
      </p>
    </div>

    <p v-if="selectedRound" class="text-muted-foreground text-xs" data-testid="round-commit">
      R{{ selectedRound.round }} は
      <a
        v-if="selectedRoundCommitHref"
        class="text-link font-mono underline underline-offset-2"
        :href="selectedRoundCommitHref"
        :title="selectedRound.head_sha"
        target="_blank"
        rel="noopener noreferrer"
        >{{ selectedRound.head_sha.slice(0, 7) }}</a
      ><span v-else class="font-mono" :title="selectedRound.head_sha">{{
        selectedRound.head_sha.slice(0, 7)
      }}</span>
      を見ています · {{ selectedRound.reviewer.username
      }}<template v-if="now">
        ·
        <time :datetime="selectedRound.created_at">{{
          reviewedAgo(selectedRound.created_at, now)
        }}</time></template
      >
    </p>

    <Alert
      v-if="roundsFailed"
      message="ラウンドを読み込めませんでした"
      retry-label="再試行"
      :on-retry="() => void roundsQuery.refetch()"
      data-testid="rounds-error"
    />

    <p
      v-if="roundMissing"
      role="status"
      class="text-muted-foreground text-sm"
      data-testid="missing-round"
    >
      URL で指定された Round {{ roundFilter }} はこの PR にありません。
    </p>
    <p
      v-if="selectedFindingMissing"
      role="status"
      class="text-muted-foreground text-sm"
      data-testid="missing-finding"
    >
      URL で指定された指摘 {{ selectedFindingId }} はこの PR にありません。
    </p>
    <p
      v-else-if="selectedFindingFiltered"
      role="status"
      class="text-muted-foreground text-sm"
      data-testid="filtered-finding"
    >
      URL で指定された指摘は現在の絞り込みでは非表示です。
    </p>

    <!-- 指摘 -->
    <div
      v-if="findingsQuery.isPending.value || reviewerWaiting"
      class="flex flex-col gap-2"
      data-testid="findings-loading"
    >
      <Skeleton v-for="n in 3" :key="n" height="2.75rem" />
    </div>
    <LoadErrorState
      v-else-if="findingsQuery.isError.value"
      message="指摘の読み込みに失敗しました"
      @retry="findingsQuery.refetch()"
    />
    <div
      v-else-if="allFindings.length === 0"
      class="bg-muted flex flex-col items-center gap-3 rounded-lg px-4 py-8 text-center"
      data-testid="no-findings"
    >
      <p class="text-sm font-semibold">この PR に指摘はありません</p>
      <Button v-if="!isComposerOpen" type="button" label="指摘を起票" @click="openComposer">
        <template #icon><PhPlus class="size-4" /></template>
      </Button>
    </div>
    <p
      v-else-if="reviewerUnavailable"
      role="status"
      class="text-muted-foreground text-sm"
      data-testid="reviewer-unavailable"
    >
      ラウンドを読み込めていないため、レビュアーで絞り込めません。
    </p>
    <div
      v-else-if="visibleFindings.length === 0"
      class="bg-muted flex flex-col items-center gap-3 rounded-lg px-4 py-8 text-center"
      data-testid="no-finding-match"
    >
      <p class="text-sm font-semibold">条件に合う指摘はありません</p>
      <Button
        v-if="hasFilters"
        type="button"
        variant="tertiary"
        label="条件を外す"
        @click="clearFilters"
      />
    </div>

    <div v-else class="flex flex-col gap-4 md:flex-row md:items-start">
      <div class="bg-card w-full shrink-0 rounded-lg border p-2 md:w-60" data-testid="file-tree">
        <Tree
          label="指摘のあるファイル"
          :heading="`ファイル · ${fileTree.fileCount}`"
          :nodes="fileTree.nodes"
          :selected-id="selectedNodeId"
          leaf-icon="file"
          :on-select="onTreeSelect"
        />
      </div>

      <div class="flex min-w-0 flex-1 flex-col gap-4" data-testid="finding-list">
        <section
          v-for="(group, index) in groups"
          :id="groupElementId(index)"
          :key="group.file ?? ''"
          class="bg-card scroll-mt-4 overflow-hidden rounded-lg border"
          :aria-label="group.file ?? 'ファイルの指定なし'"
          data-testid="file-group"
        >
          <h2 class="bg-muted flex items-center gap-2 border-b px-3 py-2 text-sm font-normal">
            <PhFile class="text-muted-foreground size-4 shrink-0" aria-hidden="true" />
            <span v-if="group.file" class="min-w-0 flex-1 truncate font-mono text-xs">
              {{ group.file }}
            </span>
            <span v-else class="min-w-0 flex-1 truncate text-xs">ファイルの指定なし</span>
            <span
              class="shrink-0 text-xs"
              :class="PROGRESS_CLASS[group.progress.tone]"
              data-testid="file-progress"
            >
              解決 {{ group.progress.resolved }} / {{ group.progress.total }}
            </span>
          </h2>
          <ul>
            <ReviewFindingRow
              v-for="finding in group.findings"
              :key="finding.id"
              :finding="finding"
              :selected="finding.id === selectedFindingId"
              :href="findingHref(finding.id)"
              :viewer-id="viewerId"
              :tenant-id="tenantId"
              :tenant-slug="tenantSlug"
              :project-id="projectId"
              :project-key="projectKey"
              :pending="transitioning"
              :error="transitionError?.id === finding.id ? transitionError.message : null"
              @toggle="toggleFinding(finding.id)"
              @transition="(to, note) => transition(finding, to, note)"
            />
          </ul>
        </section>
      </div>
    </div>

    <ConfirmDialog
      :open="confirmingLeave"
      title="下書きを破棄しますか？"
      message="起票中の下書きは、一覧へ戻ると失われます。"
      confirm-label="破棄して戻る"
      destructive
      :on-confirm="confirmLeave"
      :on-cancel="() => (confirmingLeave = false)"
    />
  </div>
</template>

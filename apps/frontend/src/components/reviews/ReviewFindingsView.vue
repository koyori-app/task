<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { PhCheckCircle, PhPlus, PhWarningCircle } from '@phosphor-icons/vue';
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import ReviewFindingBody from '@/components/reviews/ReviewFindingBody.vue';
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
import {
  SEVERITIES,
  SEVERITY_LABELS,
  STATES,
  STATE_LABELS,
  findingActions,
  findingLocation,
  mergeVerdict,
  sortFindings,
  summaryRows,
  type FindingSeverity,
  type FindingState,
  type Review,
  type ReviewFinding,
} from '@/lib/review-findings';
import {
  DEFAULT_REVIEW_FINDINGS_URL_STATE,
  applyReviewFindingsUrlState,
  parseReviewFindingsUrlState,
  relativeReviewFindingHref,
  type ReviewFindingsUrlState,
} from '@/lib/review-findings-url-state';

const props = defineProps<{
  tenantId: string;
  tenantSlug: string;
  projectId: string;
  projectKey: string;
  /** 現在の利用者。自分の修正を自分で確認できない旨の説明を出すのに使う */
  viewerId: string;
  /** SSR が URL から復元した初期表示。 */
  initialUrlState?: ReviewFindingsUrlState;
  /** URL の不正値を黙って捨てず、利用者へ知らせる。 */
  initialUrlWarnings?: string[];
  /**
   * 現在の URL（pathname + search）。SSR で findingHref を組む土台に使い、
   * この画面が持たぬ query（tab= の類）を SSR 出力の链でも保つ。
   */
  requestUrl?: string | null;
}>();

const queryClient = useQueryClient();

const initialUrlState = props.initialUrlState ?? DEFAULT_REVIEW_FINDINGS_URL_STATE;
const selectedPr = ref<number | null>(initialUrlState.pr);
const roundFilter = ref<number | null>(initialUrlState.round);
const severityFilter = ref<FindingSeverity | null>(initialUrlState.severity);
const stateFilter = ref<FindingState | null>(initialUrlState.state);
const focusedFindingId = ref<string | null>(initialUrlState.finding);
const urlWarnings = ref([...(props.initialUrlWarnings ?? [])]);
const isComposerOpen = ref(false);
const transitionError = ref<string | null>(null);

const prsQuery = useReviewedPullRequestsQuery(props.tenantId, props.projectId);
const roundsQuery = useReviewRoundsQuery(props.tenantId, props.projectId, selectedPr);
const findingsQuery = useReviewFindingsQuery(props.tenantId, props.projectId, selectedPr);
const summaryQuery = useReviewSummaryQuery(props.tenantId, props.projectId, selectedPr);
const updateState = useUpdateFindingStateMutation();

const pullRequests = computed(() => prsQuery.data.value ?? []);
/** PR 一覧の番号絞り（部分一致）。選択中 PR とは別——サイドバーの見える行だけを絞る。 */
const prNumberFilter = ref('');
const filteredPullRequests = computed(() => {
  const query = prNumberFilter.value.trim();
  if (query === '') return pullRequests.value;
  return pullRequests.value.filter((pr) => String(pr.pr_number).includes(query));
});
const rounds = computed(() => roundsQuery.data.value ?? []);

function currentUrlState(): ReviewFindingsUrlState {
  return {
    pr: selectedPr.value,
    round: roundFilter.value,
    severity: severityFilter.value,
    state: stateFilter.value,
    finding: focusedFindingId.value,
  };
}

function writeUrl(mode: 'push' | 'replace') {
  if (import.meta.env.SSR) return;
  const next = applyReviewFindingsUrlState(new URL(window.location.href), currentUrlState());
  window.history[mode === 'push' ? 'pushState' : 'replaceState'](window.history.state, '', next);
}

/** popstate・vike の遷移（props の更新）・URL 直読みの三経路が集まる唯一の復元口。 */
function applyUrlState(state: ReviewFindingsUrlState, warnings: readonly string[]) {
  selectedPr.value = state.pr;
  roundFilter.value = state.round;
  severityFilter.value = state.severity;
  stateFilter.value = state.state;
  focusedFindingId.value = state.finding;
  urlWarnings.value = [...warnings];
  isComposerOpen.value = false;
  transitionError.value = null;
}

function restoreFromLocation() {
  const parsed = parseReviewFindingsUrlState(new URL(window.location.href).searchParams);
  applyUrlState(parsed.state, parsed.warnings);
}

onMounted(() => window.addEventListener('popstate', restoreFromLocation));
onUnmounted(() => window.removeEventListener('popstate', restoreFromLocation));

// vike は同じ +Page.vue に解決される URL 間の遷移で component を差し替えず patch する
// （+Page.vue の :key の註）。setup で読んだ initialUrlState はそのままでは凍るため、
// props の更新も同じ復元口へ流す。writeUrl は pageContext を動かさないので環はできない。
// 値が現状と同じなら何もしない（復元は composer や transitionError まで畳むため）。
watch(
  () => [props.initialUrlState, props.initialUrlWarnings] as const,
  ([state, warnings]) => {
    const next = state ?? DEFAULT_REVIEW_FINDINGS_URL_STATE;
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

const summary = computed(() => summaryQuery.data.value ?? null);

/** 初期 PR の指定が無ければ、最後にレビューされた PR を開く。 */
watch(pullRequests, (list) => {
  if (selectedPr.value === null && list.length > 0) {
    selectedPr.value = list[0].pr_number;
    writeUrl('replace');
  }
});

/** 絞り込みは画面側で適用する（API は PR 単位で全件返す）。 */
const allFindings = computed(() => sortFindings(findingsQuery.data.value ?? []));
const findings = computed(() =>
  allFindings.value.filter(
    (finding) =>
      (roundFilter.value === null || finding.round === roundFilter.value) &&
      (severityFilter.value === null || finding.severity === severityFilter.value) &&
      (stateFilter.value === null || finding.state === stateFilter.value),
  ),
);

const hasFilters = computed(
  () => roundFilter.value !== null || severityFilter.value !== null || stateFilter.value !== null,
);

const selectedPrMissing = computed(
  () =>
    !prsQuery.isPending.value &&
    selectedPr.value !== null &&
    !pullRequests.value.some((pr) => pr.pr_number === selectedPr.value),
);
const roundMissing = computed(
  () =>
    !roundsQuery.isPending.value &&
    roundFilter.value !== null &&
    !rounds.value.some((round: Review) => round.round === roundFilter.value),
);
const focusedFindingMissing = computed(
  () =>
    !findingsQuery.isPending.value &&
    focusedFindingId.value !== null &&
    !allFindings.value.some((finding) => finding.id === focusedFindingId.value),
);
const focusedFindingFiltered = computed(
  () =>
    focusedFindingId.value !== null &&
    !focusedFindingMissing.value &&
    !findings.value.some((finding) => finding.id === focusedFindingId.value),
);

function resetFilters() {
  roundFilter.value = null;
  severityFilter.value = null;
  stateFilter.value = null;
}

/**
 * 利用者の操作で URL を書き直した時に呼ぶ。書き直した時点で不正値は URL から
 * 消えているため、出しっぱなしだった警告の帯も一緒に片付ける。
 * （読み込み時の自動 PR 選択のような、操作でない書き換えでは呼ばない——
 * 入場時の警告を読む前に消してしまうため。）
 */
function clearUrlWarnings() {
  urlWarnings.value = [];
}

function clearFilters() {
  resetFilters();
  focusedFindingId.value = null;
  clearUrlWarnings();
  writeUrl('replace');
}

function setRoundFilter(value: unknown) {
  roundFilter.value = value === 'all' || value == null ? null : Number(value);
  focusedFindingId.value = null;
  clearUrlWarnings();
  writeUrl('replace');
}

function setSeverityFilter(value: unknown) {
  severityFilter.value = value === 'all' || value == null ? null : (value as FindingSeverity);
  focusedFindingId.value = null;
  clearUrlWarnings();
  writeUrl('replace');
}

function setStateFilter(value: unknown) {
  stateFilter.value = value === 'all' || value == null ? null : (value as FindingState);
  focusedFindingId.value = null;
  clearUrlWarnings();
  writeUrl('replace');
}

function selectPr(pr: number) {
  selectedPr.value = pr;
  resetFilters();
  focusedFindingId.value = null;
  isComposerOpen.value = false;
  transitionError.value = null;
  clearUrlWarnings();
  writeUrl('push');
}

function findingHref(id: string): string {
  // SSR は props.requestUrl（+Page が pageContext から渡す現在 URL）を土台にする。
  // path を自前で組むと、この画面が持たぬ query（tab= の類）が SSR 出力の链から
  // 落ちて client と食い違う。出力は relativeReviewFindingHref で両側とも
  // pathname + search の相対形に揃える（client だけ絶対 URL だと、同じ属性が
  // hydration で別の文字列になる）。
  const base = import.meta.env.SSR
    ? new URL(props.requestUrl ?? '/', 'http://ssr.local')
    : new URL(window.location.href);
  return relativeReviewFindingHref(base, currentUrlState(), id);
}

async function focusFinding(event: MouseEvent, id: string) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  event.preventDefault();
  focusedFindingId.value = id;
  clearUrlWarnings();
  writeUrl('replace');
  await nextTick();
  document.getElementById(`finding-${id}`)?.scrollIntoView?.({ block: 'center' });
  scrolledFindingId.value = id;
}

/**
 * 注目指摘への強制スクロールは、注目が変わった時の一度きり。済んだ id を覚え、
 * 状態変更 → invalidate → 再取得で findings の data が入れ替わるたびに
 * scrollIntoView が走って画面が勝手に戻るのを防ぐ。data も依存に残すのは、
 * 共有 link 直入りでは data 到着まで要素が無く、到着後の再走で初回の
 * スクロールを果たすため（要素が出るまでは済み扱いにしない）。
 */
const scrolledFindingId = ref<string | null>(null);
watch(
  () => [focusedFindingId.value, findingsQuery.data.value] as const,
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

async function invalidateAll() {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['get', REVIEWED_PRS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', REVIEWS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', REVIEW_FINDINGS_PATH] }),
    queryClient.invalidateQueries({ queryKey: ['get', REVIEW_SUMMARY_PATH] }),
  ]);
}

/**
 * サーバーが本文に入れた理由。読ませる意味のあるものだけ返す。
 *
 * 共通のエラー本文は `conflict` / `forbidden` のようなスラグで、そのまま出しても
 * 利用者は何も判断できない。理由付きの 409（High の繰り延べなど）だけを拾う。
 */
function serverReason(message: string | undefined): string | null {
  if (!message || /^[a-z][a-z-]*$/.test(message)) return null;
  return message;
}

async function transition(finding: ReviewFinding, to: FindingState) {
  transitionError.value = null;
  try {
    await updateState.mutateAsync({
      params: {
        path: { tenant_id: props.tenantId, project_id: props.projectId, id: finding.id },
      },
      body: { state: to, note: null },
    });
    await invalidateAll();
  } catch (e) {
    const err = e as { response?: { status?: number }; error?: { message?: string } };
    const status = err.response?.status;
    transitionError.value =
      serverReason(err.error?.message) ??
      (status === 403
        ? 'この操作はレビュー側だけが行えます（自分の修正は自分で確認できません）。'
        : status === 409
          ? 'いまの状態からは行えない操作です。画面を再読み込みしてください。'
          : '状態を更新できませんでした。');
    // 失敗した操作の結果が画面に残らないよう、正しい状態を取り直す
    await invalidateAll();
  }
}

async function onRoundCreated() {
  isComposerOpen.value = false;
  await invalidateAll();
}
</script>

<template>
  <div class="mx-auto flex w-full max-w-[1080px] flex-col gap-6">
    <div class="flex flex-col gap-1">
      <h1 class="text-2xl font-bold tracking-tight">レビュー指摘</h1>
      <p class="text-muted-foreground text-sm">
        指摘の一覧と状態はここが権威です。GitHub の PR には要約コメントが 1 本だけ置かれます。
      </p>
    </div>

    <p v-if="prsQuery.isPending.value" class="text-muted-foreground text-sm">読み込み中…</p>
    <p v-else-if="prsQuery.isError.value" role="alert" class="text-destructive text-sm">
      レビューを読み込めませんでした
    </p>

    <template v-else>
      <div
        v-if="urlWarnings.length > 0"
        role="status"
        class="bg-muted text-muted-foreground rounded-md border p-3 text-sm"
        data-testid="url-warning"
      >
        <p v-for="warning in urlWarnings" :key="warning">{{ warning }}</p>
      </div>

      <div class="flex flex-col gap-6 md:flex-row md:items-start">
        <!-- PR 一覧 -->
        <nav
          class="flex w-full shrink-0 flex-col gap-1 md:w-[280px]"
          aria-label="レビューのある PR"
        >
          <div v-if="pullRequests.length > 0" class="mb-2">
            <label for="filter-pr-number" class="mb-1.5 block text-xs font-medium">PR 番号</label>
            <Input
              id="filter-pr-number"
              v-model="prNumberFilter"
              inputmode="numeric"
              autocomplete="off"
              placeholder="部分一致"
              class="font-mono text-sm"
              data-testid="filter-pr-number"
            />
          </div>
          <p v-if="pullRequests.length === 0" class="text-muted-foreground text-sm">
            レビューはまだありません。
          </p>
          <p
            v-else-if="prNumberFilter.trim() !== '' && filteredPullRequests.length === 0"
            class="text-muted-foreground text-sm"
            data-testid="no-pr-match"
          >
            該当する PR がありません。
          </p>
          <button
            v-for="pr in filteredPullRequests"
            :key="pr.pr_number"
            type="button"
            class="flex flex-col gap-1 rounded-md border px-3 py-2 text-left text-sm"
            :class="
              pr.pr_number === selectedPr
                ? 'bg-accent border-accent-foreground/20'
                : 'hover:bg-accent/50'
            "
            :aria-current="pr.pr_number === selectedPr ? 'true' : undefined"
            @click="selectPr(pr.pr_number)"
          >
            <span class="flex items-center gap-2">
              <span class="font-mono text-xs">#{{ pr.pr_number }}</span>
              <!--
                バッジは件数の事実だけを出す。可否の断定はしない——可否には鮮度と
                連携の有無も要り（mergeVerdict の片道降格）、この一覧はその材料を
                持たない。「マージ可」を出すと、右の判定パネルが「リポジトリ未確定」
                等と言う横で一覧だけ緑になる
              -->
              <span
                class="rounded-full px-2 py-0.5 text-xs"
                :class="
                  pr.blocking === 0
                    ? 'bg-green-600/10 text-green-700 dark:text-green-400'
                    : 'bg-destructive/10 text-destructive'
                "
              >
                {{ pr.blocking === 0 ? '未解決なし' : `${pr.blocking} 件が未解決` }}
              </span>
            </span>
            <span class="truncate font-medium">{{ pr.pr_title ?? `PR #${pr.pr_number}` }}</span>
            <span class="text-muted-foreground text-xs">
              <template v-if="pr.pr_author">{{ pr.pr_author }} · </template>R{{ pr.rounds }} ·
              未解決 {{ pr.unresolved }}
            </span>
          </button>
        </nav>

        <!-- 指摘 -->
        <div class="flex min-w-0 flex-1 flex-col gap-4">
          <p v-if="selectedPr === null" class="text-muted-foreground text-sm">
            PR を選ぶと指摘が表示されます。
          </p>

          <template v-else>
            <!-- マージ判定 -->
            <div
              v-if="summary"
              class="flex items-center gap-3 rounded-lg border p-4"
              :class="
                mergeVerdict(summary).kind === 'ready'
                  ? 'border-green-600/40 bg-green-600/5'
                  : 'border-destructive/40 bg-destructive/5'
              "
              data-testid="merge-gate"
            >
              <component
                :is="mergeVerdict(summary).kind === 'ready' ? PhCheckCircle : PhWarningCircle"
                class="size-5 shrink-0"
                :class="
                  mergeVerdict(summary).kind === 'ready'
                    ? 'text-green-700 dark:text-green-400'
                    : 'text-destructive'
                "
              />
              <div class="flex flex-col">
                <span class="text-sm font-medium">{{ mergeVerdict(summary).title }}</span>
                <span class="text-muted-foreground text-xs">
                  {{ mergeVerdict(summary).detail }}
                </span>
                <span
                  v-if="summary.repository || summary.owner_override_rejections > 0"
                  class="text-muted-foreground mt-0.5 text-xs"
                >
                  <template v-if="summary.repository">{{ summary.repository }}</template>
                  <template v-if="summary.owner_override_rejections > 0">
                    · オーナー代行での棄却 {{ summary.owner_override_rejections }} 件
                  </template>
                </span>
              </div>
              <div class="ml-auto flex flex-wrap justify-end gap-1">
                <span
                  v-for="row in summaryRows(summary)"
                  :key="`${row.severity}-${row.state}`"
                  class="text-muted-foreground rounded-md border px-2 py-0.5 text-xs"
                >
                  {{ SEVERITY_LABELS[row.severity] }} / {{ STATE_LABELS[row.state] }}:
                  {{ row.count }}
                </span>
              </div>
            </div>

            <!-- 絞り込みと起票 -->
            <div class="flex flex-wrap items-end gap-2">
              <div class="w-[110px]">
                <label for="filter-round" class="mb-1.5 block text-xs font-medium">Round</label>
                <Select
                  :model-value="roundFilter === null ? 'all' : String(roundFilter)"
                  @update:model-value="setRoundFilter"
                >
                  <SelectTrigger id="filter-round" size="sm" class="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">すべて</SelectItem>
                    <SelectItem
                      v-for="round in rounds"
                      :key="round.id"
                      :value="String(round.round)"
                    >
                      R{{ round.round }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div class="w-[130px]">
                <label for="filter-severity" class="mb-1.5 block text-xs font-medium">重大度</label>
                <Select
                  :model-value="severityFilter ?? 'all'"
                  @update:model-value="setSeverityFilter"
                >
                  <SelectTrigger id="filter-severity" size="sm" class="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">すべて</SelectItem>
                    <SelectItem v-for="severity in SEVERITIES" :key="severity" :value="severity">
                      {{ SEVERITY_LABELS[severity] }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div class="w-[130px]">
                <label for="filter-state" class="mb-1.5 block text-xs font-medium">状態</label>
                <Select :model-value="stateFilter ?? 'all'" @update:model-value="setStateFilter">
                  <SelectTrigger id="filter-state" size="sm" class="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">すべて</SelectItem>
                    <SelectItem v-for="state in STATES" :key="state" :value="state">
                      {{ STATE_LABELS[state] }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <Button
                v-if="hasFilters"
                type="button"
                variant="ghost"
                size="sm"
                @click="clearFilters"
              >
                絞り込み解除
              </Button>
              <Button
                v-if="!isComposerOpen"
                type="button"
                size="sm"
                class="ml-auto"
                @click="isComposerOpen = true"
              >
                <PhPlus class="size-4" />
                指摘を起票
              </Button>
            </div>

            <ReviewRoundComposer
              v-if="isComposerOpen"
              :tenant-id="tenantId"
              :project-id="projectId"
              :pr-number="selectedPr"
              :next-round="rounds.length + 1"
              @created="onRoundCreated"
              @cancel="isComposerOpen = false"
            />

            <p v-if="transitionError" role="alert" class="text-destructive text-sm">
              {{ transitionError }}
            </p>

            <p
              v-if="selectedPrMissing"
              role="status"
              class="text-muted-foreground text-sm"
              data-testid="missing-pr"
            >
              URL で指定された PR #{{ selectedPr }} はレビュー一覧にありません。
            </p>
            <p
              v-if="roundMissing"
              role="status"
              class="text-muted-foreground text-sm"
              data-testid="missing-round"
            >
              URL で指定された Round {{ roundFilter }} はこの PR にありません。
            </p>
            <p
              v-if="focusedFindingMissing"
              role="status"
              class="text-muted-foreground text-sm"
              data-testid="missing-finding"
            >
              URL で指定された指摘 {{ focusedFindingId }} はこの PR にありません。
            </p>
            <p
              v-else-if="focusedFindingFiltered"
              role="status"
              class="text-muted-foreground text-sm"
              data-testid="filtered-finding"
            >
              URL で指定された指摘は現在の絞り込みでは非表示です。
            </p>

            <p v-if="findingsQuery.isPending.value" class="text-muted-foreground text-sm">
              読み込み中…
            </p>
            <p v-else-if="findings.length === 0" class="text-muted-foreground text-sm">
              {{ hasFilters ? '条件に合う指摘はありません。' : '指摘はありません。' }}
            </p>

            <ul v-else class="flex flex-col gap-3" data-testid="finding-list">
              <li
                v-for="finding in findings"
                :id="`finding-${finding.id}`"
                :key="finding.id"
                class="rounded-lg border p-4"
                :class="
                  finding.id === focusedFindingId
                    ? 'border-primary ring-primary/40 ring-2'
                    : undefined
                "
                :aria-current="finding.id === focusedFindingId ? 'true' : undefined"
                :data-focused="finding.id === focusedFindingId ? 'true' : undefined"
              >
                <div class="flex flex-wrap items-center gap-2">
                  <span
                    class="rounded-md border px-2 py-0.5 text-xs font-medium"
                    :class="
                      finding.severity === 'high' || finding.severity === 'medium'
                        ? 'border-destructive/40 text-destructive'
                        : 'text-muted-foreground'
                    "
                  >
                    {{ SEVERITY_LABELS[finding.severity] }}
                  </span>
                  <span class="text-muted-foreground rounded-md border px-2 py-0.5 text-xs">
                    {{ STATE_LABELS[finding.state] }}
                  </span>
                  <span class="text-muted-foreground text-xs">R{{ finding.round }}</span>
                  <a
                    class="hover:text-primary min-w-0 flex-1 truncate font-medium underline-offset-4 hover:underline"
                    :href="findingHref(finding.id)"
                    :aria-label="`指摘「${finding.title}」へのリンク`"
                    @click="focusFinding($event, finding.id)"
                  >
                    {{ finding.title }}
                  </a>
                </div>

                <p
                  v-if="findingLocation(finding)"
                  class="text-muted-foreground mt-1 font-mono text-xs"
                >
                  {{ findingLocation(finding) }}
                </p>
                <ReviewFindingBody :body="finding.body" :finding-id="finding.id" />

                <div class="mt-3 flex flex-wrap items-center gap-2">
                  <Button
                    v-for="action in findingActions(finding)"
                    :key="action.to"
                    type="button"
                    size="sm"
                    :variant="action.to === 'verified' ? 'default' : 'outline'"
                    :disabled="updateState.isPending.value"
                    @click="transition(finding, action.to)"
                  >
                    {{ action.label }}
                  </Button>
                  <!-- 確認ボタンが出ない理由の説明だけ。出すかどうかは available_actions が決める -->
                  <span
                    v-if="finding.state === 'fixed' && finding.fixed_by === viewerId"
                    class="text-muted-foreground text-xs"
                  >
                    修正者と確認者は別の人である必要があります
                  </span>
                </div>

                <details v-if="finding.transitions.length > 0" class="mt-3">
                  <summary class="text-muted-foreground cursor-pointer text-xs">遷移履歴</summary>
                  <ul class="mt-2 flex flex-col gap-1">
                    <li
                      v-for="history in finding.transitions"
                      :key="history.id"
                      class="text-muted-foreground text-xs"
                    >
                      {{ history.actor.username }} ·
                      {{
                        history.from_state
                          ? `${STATE_LABELS[history.from_state]} → ${STATE_LABELS[history.to_state]}`
                          : `${STATE_LABELS[history.to_state]} として登録`
                      }}
                      <template v-if="history.note"> · {{ history.note }}</template>
                    </li>
                  </ul>
                </details>
              </li>
            </ul>
          </template>
        </div>
      </div>
    </template>
  </div>
</template>

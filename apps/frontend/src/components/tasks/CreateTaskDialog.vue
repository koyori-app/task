<script setup lang="ts">
import {
  ChevronRight,
  CircleDashed,
  Flag,
  Folder,
  Percent,
  Tag,
  Timer,
  User,
  X,
} from '@lucide/vue';
import { computed, onUnmounted, ref, watch } from 'vue';
import { useQueryClient } from '@tanstack/vue-query';
import { getActivePinia } from 'pinia';
import { usePageContext } from 'vike-vue/usePageContext';

import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import {
  Button,
  ConfirmDialog,
  Dialog,
  Dropdown,
  Picker,
  Spinner,
  type DropdownItem,
} from '@koyori-app/ui-vue';
import UserSelector from '@/components/users/UserSelector.vue';
import MarkdownEditor from '@/components/markdown/MarkdownEditor.vue';
import SimilarTasksList from '@/components/tasks/SimilarTasksList.vue';
import TaskDatesChip from '@/components/tasks/TaskDatesChip.vue';
import TaskNumberChip from '@/components/tasks/TaskNumberChip.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { useSimilarTasks } from '@/composables/useSimilarTasks';
import type { components } from '@/generated/api';
import { apiClient, TASK_SEARCH_PATH } from '@/lib/api-vue-query';
import { PRIORITY_CONFIG, taskDetailHref } from '@/lib/task-display';
import { chipTrigger } from '@/lib/task-chip';
import { toIsoDate } from '@/lib/task-date';
import { useAuthStore } from '@/stores/auth';

const CREATE_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

type Priority = components['schemas']['TaskPriority'];
type Status = components['schemas']['ProjectStatusResponse'];
type LabelOption = components['schemas']['LabelResponse'];
/** 担当者の候補。`assignable-users` が返すのはメンバー行ではなく利用者そのもの */
type MemberOption = components['schemas']['AssignableUserResponse'];
type CreatedTask = components['schemas']['TaskDetailResponse'];

/** 担当者に付ける役割。詳細から付けるときと同じ値にする（docs/dev/features/tasks/1.core.md）。 */
const ASSIGNEE_ROLE = 'primary';

/**
 * 見積もり（分）の上限。API の `estimated_minutes` は i32 なので、超えると backend の
 * デシリアライズで落ち、利用者には入力の誤りではなく「作成に失敗しました」しか出ない。
 */
const MAX_ESTIMATE_MINUTES = 2_147_483_647;

/** 進捗（%）の上限。API の `progress_pct` は 0 以上 100 以下の整数 */
const MAX_PROGRESS_PCT = 100;

/** 似ているタスクの問い合わせを待つ時間。一覧の検索（tasks/+Page.vue）と揃える */
const TITLE_DEBOUNCE_MS = 300;

/**
 * 優先度の Picker の項目。`icon: true` は koyori に行頭の `#item-icon` スロットを出させる目印で、
 * 旗の色はスロットの中で PRIORITY_CONFIG から引く（色の点を出す `color` は渡さない。点と旗が並んでしまう）。
 * 引き金のチップと同じ旗
 */
const PRIORITY_ITEMS = (Object.keys(PRIORITY_CONFIG) as Priority[]).map((value) => ({
  value,
  label: PRIORITY_CONFIG[value].label,
  icon: true,
}));

const props = defineProps<{
  open: boolean;
  tenantId: string;
  projectId: string;
  projectKey: string;
  statuses: Status[];
  /** undefined は未取得（ロード中・エラー）。正常な 0 件は空配列で渡すこと */
  labels?: LabelOption[];
  labelsLoading?: boolean;
  /** ラベル一覧が手元に無いときだけ true にすること（使えるキャッシュがあれば false） */
  labelsError?: boolean;
  /**
   * 担当者に選べる利用者。undefined は未取得（ロード中・エラー）。正常な 0 人は空配列で渡すこと。
   *
   * 取得先は `assignable-users`。`members` は管理者しか読めないので、担当者を付けられる
   * 権限（WriteTask）しか持たない利用者では候補が引けない
   */
  members?: MemberOption[];
  membersLoading?: boolean;
  /** 候補が手元に無いときだけ true にすること（使えるキャッシュがあれば false。labelsError と同じ約束） */
  membersError?: boolean;
  /** 開いたときのタイトルの初期値（ドキュメントの選択範囲から作るとき等） */
  initialTitle?: string;
  /**
   * 開いたときの担当者の初期値（ホームから作るときは自分）。
   *
   * 候補（members）を取得してから、候補にある ID だけを入れる。候補が未取得の間は入れない
   * （担当者に指定できない利用者を送って 403 になるのを避ける）。利用者が担当者を変えたあとは上書きしない
   */
  initialAssigneeIds?: string[];
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  created: [task: CreatedTask];
  retryLabels: [];
  retryMembers: [];
}>();

const queryClient = useQueryClient();
const title = ref('');
const statusId = ref('');
const description = ref('');
const plannedStartOn = ref('');
const softDeadline = ref('');
const hardDeadline = ref('');
/** 見積もり（分）。自前の number 入力は文字列で返す。空は未入力 */
const estimate = ref('');
/** 進捗（%）。見積もりと同じく文字列で持つ。空は未入力（送らず、サーバーの既定 0 に任せる） */
const progress = ref('');
const priority = ref<Priority>('Medium');
const selectedLabelIds = ref<string[]>([]);
const selectedAssigneeIds = ref<string[]>([]);
/** 候補と照合して入れた初期担当者。dirty の基準で、候補が届くまでは空 */
const appliedAssigneeIds = ref<string[]>([]);

/** 並びに依らず同じ ID の集合か */
function sameIds(a: string[], b: string[]) {
  return a.length === b.length && a.every((id) => b.includes(id));
}

/**
 * 初期担当者を、候補にある ID だけ選択へ入れる。候補が未取得、または閉じているときは何もしない。
 * 選択が入れた値のままの間（利用者がまだ触っていない間）だけ入れ直す。触ったあとは上書きしない
 */
function applyInitialAssignees() {
  if (!props.open || !props.members) return;
  if (!sameIds(selectedAssigneeIds.value, appliedAssigneeIds.value)) return;
  const known = new Set(props.members.map((user) => user.id));
  const ids = (props.initialAssigneeIds ?? []).filter((id) => known.has(id));
  appliedAssigneeIds.value = ids;
  selectedAssigneeIds.value = [...ids];
}
const validationMessage = ref<string | null>(null);
// 文字列（YYYY-MM-DD）同士の比較で足りる。サーバーは前後関係を検証しない
const plannedStartAfterSoft = computed(
  () => !!plannedStartOn.value && !!softDeadline.value && plannedStartOn.value > softDeadline.value,
);
const requestError = ref<string | null>(null);
const successMessage = ref<string | null>(null);

const defaultStatusId = computed(
  () => props.statuses.find((status) => status.is_default)?.id ?? props.statuses[0]?.id ?? '',
);
const statusItems = computed(() =>
  props.statuses.map((status) => ({ value: status.id, label: status.name, color: status.color })),
);
const selectedStatus = computed(() =>
  props.statuses.find((status) => status.id === statusId.value),
);

const selectedLabels = computed(() =>
  (props.labels ?? []).filter((label) => selectedLabelIds.value.includes(label.id)),
);
const labelItems = computed<DropdownItem[]>(() =>
  (props.labels ?? []).map((label) => ({
    value: label.id,
    label: label.name,
    kind: 'checkbox',
    checked: selectedLabelIds.value.includes(label.id),
  })),
);
// 候補で自分を先頭に置く。pinia の無い所（試験）でも組めるよう、在る時だけ引く。
const authStore = getActivePinia() ? useAuthStore() : null;
const currentUserId = computed(() => authStore?.user?.id ?? null);

const selectedAssignees = computed(() =>
  (props.members ?? []).filter((user) => selectedAssigneeIds.value.includes(user.id)),
);

const createMutation = apiClient.useMutation('post', CREATE_TASK_PATH);
const titleInputRef = ref<HTMLInputElement | null>(null);

// 開いて入力欄ができたら当てる（× より先に）
watch(titleInputRef, (input) => {
  input?.focus();
});

// 似ているタスク（重複の注意喚起）。作成は妨げない: 取得中・失敗・0 件は何も出さない
const pageContext = usePageContext();
// ダイアログは API 用の UUID しか受け取らないので、リンクの表示用 ID はルートから取る
const tenantDisplayId = computed(() => String(pageContext.routeParams.tenant ?? ''));
const debouncedTitle = ref('');
let titleDebounceTimer: ReturnType<typeof setTimeout> | undefined;
watch(title, (value) => {
  if (titleDebounceTimer) clearTimeout(titleDebounceTimer);
  titleDebounceTimer = setTimeout(() => {
    debouncedTitle.value = value.trim();
  }, TITLE_DEBOUNCE_MS);
});
onUnmounted(() => {
  if (titleDebounceTimer) clearTimeout(titleDebounceTimer);
});
const similarQuery = useSimilarTasks({
  tenantId: () => props.tenantId,
  projectId: () => props.projectId,
  title: debouncedTitle,
});
// debounce 中は入力と問い合わせがずれるので、追いつくまで旧タイトルの候補を出さない
const similarTasks = computed(() =>
  props.open && debouncedTitle.value === title.value.trim()
    ? (similarQuery.data.value?.tasks ?? [])
    : [],
);

// 重複を確かめるために押すので、ダイアログも入力も残したまま別タブで開く
function openSimilarTask(seqId: number) {
  window.open(taskDetailHref(tenantDisplayId.value, props.projectKey, seqId), '_blank', 'noopener');
}

watch(
  () => [props.open, defaultStatusId.value] as const,
  ([open]) => {
    if (open && !props.statuses.some((status) => status.id === statusId.value)) {
      statusId.value = defaultStatusId.value;
    }
  },
  { immediate: true },
);

// 開くたびに初期値を入れる。閉じると resetForm が空に戻す
watch(
  () => props.open,
  (open) => {
    if (open && props.initialTitle) title.value = props.initialTitle;
    if (open) {
      selectedAssigneeIds.value = [];
      appliedAssigneeIds.value = [];
      applyInitialAssignees();
    }
  },
  { immediate: true },
);

// プロジェクトが切り替わったら旧プロジェクトの入力（特にラベル ID）を持ち越さない
watch(
  () => props.projectId,
  () => resetForm(),
);

// ラベル一覧の正常取得後、削除済み ID を選択から落とす。
// undefined はロード中・エラー（一覧が不明）なので選択を保持する
watch(
  () => props.labels,
  (labels) => {
    if (!labels) return;
    const ids = new Set(labels.map((label) => label.id));
    selectedLabelIds.value = selectedLabelIds.value.filter((id) => ids.has(id));
  },
);

// 候補の正常取得後、初期担当者を入れ（触っていなければ入れ直し）、外れた ID を選択から落とす（ラベルと同じ理由）
watch(
  () => props.members,
  (members) => {
    if (!members) return;
    applyInitialAssignees();
    const ids = new Set(members.map((user) => user.id));
    selectedAssigneeIds.value = selectedAssigneeIds.value.filter((id) => ids.has(id));
  },
);

// 閉じたら入力と結果表示を捨てる。
//
// onClose 側だけで捨てていたときは、作成に成功して親が open を false にする
// 経路（created を受けてダイアログを閉じる）がここを通らないため、「タスクを
// 作成しました」が残り、次に開いた瞬間に前回の成功が表示されていた。
watch(
  () => props.open,
  (open) => {
    if (!open) resetForm();
  },
);

/**
 * 入力があるか。フォームの全項目を、開いた直後の値（title は initialTitle、他は resetForm が入れる値）と
 * 比べる（担当者は候補と照合して入れた初期値との差）。項目を足したら resetForm とここを揃えること。あれば閉じる前に確認する
 */
const dirty = computed(
  () =>
    title.value !== (props.initialTitle ?? '') ||
    description.value !== '' ||
    statusId.value !== defaultStatusId.value ||
    plannedStartOn.value !== '' ||
    softDeadline.value !== '' ||
    hardDeadline.value !== '' ||
    estimate.value !== '' ||
    progress.value !== '' ||
    priority.value !== 'Medium' ||
    selectedLabelIds.value.length > 0 ||
    !sameIds(selectedAssigneeIds.value, appliedAssigneeIds.value),
);
const discardConfirmOpen = ref(false);

function onClose() {
  if (createMutation.isPending.value) return;
  if (dirty.value) {
    discardConfirmOpen.value = true;
    return;
  }
  closeDialog();
}

function cancelDiscard() {
  discardConfirmOpen.value = false;
}

function confirmDiscard() {
  discardConfirmOpen.value = false;
  closeDialog();
}

// 日本語入力の変換中の Esc は変換の取り消しで、ダイアログを閉じる操作ではない。
// koyori の Dialog は Esc を dialog の cancel で受けるので、keydown の既定動作を止めて cancel を起こさせない
// （Firefox / Safari は変換確定の Esc でも keydown を流し、その場合 isComposing は false で keyCode が 229）
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && (event.isComposing || event.keyCode === 229)) {
    event.preventDefault();
  }
}

function closeDialog() {
  // 親が open を握っていない場合でも捨てられるよう、ここでも呼ぶ。
  // resetForm は冪等なので watch と二重に走っても問題ない
  resetForm();
  emit('update:open', false);
}

function toggleLabel(labelId: string) {
  selectedLabelIds.value = selectedLabelIds.value.includes(labelId)
    ? selectedLabelIds.value.filter((id) => id !== labelId)
    : [...selectedLabelIds.value, labelId];
}

function resetForm() {
  title.value = '';
  statusId.value = defaultStatusId.value;
  description.value = '';
  plannedStartOn.value = '';
  softDeadline.value = '';
  hardDeadline.value = '';
  estimate.value = '';
  progress.value = '';
  priority.value = 'Medium';
  selectedLabelIds.value = [];
  selectedAssigneeIds.value = [];
  appliedAssigneeIds.value = [];
  applyInitialAssignees();
  validationMessage.value = null;
  requestError.value = null;
  successMessage.value = null;
}

async function submit(event: SubmitEvent) {
  if (createMutation.isPending.value) return;

  const normalizedTitle = title.value.trim();
  validationMessage.value = null;
  requestError.value = null;
  successMessage.value = null;

  if (!normalizedTitle) {
    validationMessage.value = 'タイトルを入力してください';
    return;
  }
  if (!statusId.value) {
    validationMessage.value = 'ステータスを選択してください';
    return;
  }
  if (plannedStartAfterSoft.value) {
    validationMessage.value = '開始予定日は期限以前の日付にしてください';
    return;
  }
  const normalizedEstimate = estimate.value.trim();
  const estimatedMinutes = normalizedEstimate ? Number(normalizedEstimate) : undefined;
  // 「1e」のように数値にならない入力は value が空で届き、未入力と区別できない
  const { elements } = event.target as HTMLFormElement;
  const estimateInput = elements.namedItem('estimated_minutes') as HTMLInputElement;
  if (
    estimateInput.validity.badInput ||
    (estimatedMinutes !== undefined &&
      (!Number.isInteger(estimatedMinutes) ||
        estimatedMinutes < 1 ||
        estimatedMinutes > MAX_ESTIMATE_MINUTES))
  ) {
    validationMessage.value = `見積もりは 1 以上 ${MAX_ESTIMATE_MINUTES} 以下の整数（分）で入力してください`;
    return;
  }
  const normalizedProgress = progress.value.trim();
  const progressPct = normalizedProgress ? Number(normalizedProgress) : undefined;
  const progressInput = elements.namedItem('progress_pct') as HTMLInputElement;
  if (
    progressInput.validity.badInput ||
    (progressPct !== undefined &&
      (!Number.isInteger(progressPct) || progressPct < 0 || progressPct > MAX_PROGRESS_PCT))
  ) {
    validationMessage.value = `進捗は 0 以上 ${MAX_PROGRESS_PCT} 以下の整数（%）で入力してください`;
    return;
  }

  const body: components['schemas']['CreateTaskRequest'] = {
    title: normalizedTitle,
    status_id: statusId.value,
    priority: priority.value,
  };
  const normalizedDescription = description.value.trim();
  if (normalizedDescription) body.description = normalizedDescription;
  if (plannedStartOn.value) body.planned_start_on = plannedStartOn.value;
  if (softDeadline.value) body.soft_deadline = toIsoDate(softDeadline.value);
  if (hardDeadline.value) body.hard_deadline = toIsoDate(hardDeadline.value);
  if (estimatedMinutes !== undefined) body.estimated_minutes = estimatedMinutes;
  // 0 も送る。未入力は送らず、サーバーの既定（0）に任せる
  if (progressPct !== undefined) body.progress_pct = progressPct;
  if (selectedLabelIds.value.length) body.label_ids = selectedLabelIds.value;
  // role は仕様書が使っている primary に揃える（役割を使い分ける UI はまだ無い）
  if (selectedAssigneeIds.value.length) {
    body.assignees = selectedAssigneeIds.value.map((user_id) => ({
      user_id,
      role: ASSIGNEE_ROLE,
    }));
  }

  try {
    const created = await createMutation.mutateAsync({
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
      body,
    });
    void Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', CREATE_TASK_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', TASK_SEARCH_PATH] }),
    ]);
    emit('created', created);
    resetForm();
    successMessage.value = 'タスクを作成しました';
  } catch {
    requestError.value = 'タスクの作成に失敗しました。もう一度お試しください';
  }
}
</script>

<template>
  <div
    class="contents [--koyori-dialog-width:760px] max-md:[--koyori-dialog-height:100dvh] md:[--koyori-dialog-height:fit-content]"
    @keydown="onKeydown"
  >
    <!--
      1 列の構成: 上部バー（パンくずと ×）、本文（タイトル・似ているタスク・説明）、プロパティのチップ列、
      メッセージ、フッター。広い画面は中身に合わせて高さが縮み、あふれたら本文だけがスクロールする。
      狭い画面は高さを画面いっぱいにし、本文だけがスクロールしてチップ列とフッターが下に留まる。
      見出しは画面に出さない（パンくずが見出しの役をする）。plain の title はダイアログの名前として残る。
      広い画面の高さは fit-content にする（plain の Dialog は height: auto だと、位置指定の dialog が
      max-height まで引き伸ばされて、中身より下に余白ができる）
    -->
    <Dialog
      :open="open"
      title="新規タスク"
      :description="`${projectKey} にタスクを追加します`"
      plain
      :on-close="onClose"
    >
      <!-- 本文は開いている間だけ置く（閉じた Dialog も DOM に残るので、Markdown エディタを常駐させない） -->
      <HydrationSafeForm
        v-if="open"
        v-slot="{ isHydrated }"
        class="flex min-h-0 flex-col max-md:flex-1 md:max-h-[calc(100dvh-2rem-2px)]"
        @submit="submit"
      >
        <header class="flex h-12 shrink-0 items-center gap-1 border-b pr-2 pl-4 md:px-3">
          <div class="flex min-w-0 items-center gap-1.5 text-[0.8125rem] text-muted-foreground">
            <Folder class="size-3.5 shrink-0" aria-hidden="true" />
            <span class="truncate">{{ projectKey }}</span>
            <ChevronRight class="size-3.5 shrink-0" aria-hidden="true" />
            <strong class="font-medium text-foreground">新規タスク</strong>
          </div>
          <Button
            type="button"
            variant="ghost"
            class="ml-auto"
            ariaLabel="閉じる"
            :disabled="createMutation.isPending.value"
            @click="onClose"
          >
            <template #icon><X class="size-4" /></template>
          </Button>
        </header>

        <div
          class="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto p-4 md:px-6 md:pt-5 md:pb-3"
        >
          <input
            id="task-title"
            ref="titleInputRef"
            v-model="title"
            name="title"
            autocomplete="off"
            aria-label="タイトル（必須）"
            placeholder="タスクのタイトル"
            class="-mx-1 w-[calc(100%+0.5rem)] rounded bg-transparent px-1 py-0.5 text-xl font-semibold tracking-tight outline-none placeholder:text-muted-foreground/60 hover:bg-muted/40 focus-visible:bg-muted/40 md:text-2xl"
          />
          <SimilarTasksList
            class="rounded-md bg-muted/60 px-2.5 py-2"
            :tasks="similarTasks"
            :tenant-display-id="tenantDisplayId"
            :project-key="projectKey"
            @select="openSimilarTask"
          />
          <!-- CodeMirror の実体は contenteditable で label の for が効かないため、
               名前は editor 側の aria-label で与える -->
          <MarkdownEditor
            v-model="description"
            variant="plain"
            aria-label="説明"
            placeholder="説明を入力（Markdown で書けます）"
            :candidates="members ?? []"
            min-height-class="min-h-44"
          />
        </div>

        <!--
          チップ列は本文の外（スクロールしない位置）に置く。説明が長くてもチップが隠れず、DOM も 1 本で済む。
          狭い画面は 1 行で横にスクロールさせ、右端をフェードで示す（最後のチップが隠れたままにならないよう右を空ける）。
          横スクロールの器で一覧が切れないよう、Picker / Dropdown / UserSelector は top-layer で出す
        -->
        <div
          class="relative shrink-0 max-md:border-t max-md:after:pointer-events-none max-md:after:absolute max-md:after:inset-y-0 max-md:after:right-0 max-md:after:w-10 max-md:after:bg-linear-to-r max-md:after:from-transparent max-md:after:to-(--koyori-color-surface-raised)"
        >
          <div
            data-task-chips
            class="flex items-center gap-1.5 px-4 py-2.5 max-md:overflow-x-auto max-md:pr-10 max-md:[scrollbar-width:none] md:flex-wrap md:px-6 md:pt-0 md:pb-4"
          >
            <Picker
              top-layer
              label="ステータス"
              :icon="null"
              :searchable="false"
              :items="statusItems"
              :selected-values="statusId ? [statusId] : []"
              :on-selection-change="(values: string[]) => (statusId = values[0] ?? '')"
              :class="chipTrigger(false)"
            >
              <template #trigger>
                <span class="flex items-center gap-1.5">
                  <CircleDashed class="size-3.5 shrink-0" aria-hidden="true" />
                  {{ selectedStatus?.name ?? 'ステータス' }}
                </span>
              </template>
            </Picker>
            <input type="hidden" name="status_id" :value="statusId" />

            <!-- Field で包まない（包むと枠付きの入力欄の見た目になる）。読み上げ名は Picker が「優先度: 中」の形で付ける -->
            <Picker
              top-layer
              label="優先度"
              :icon="null"
              :searchable="false"
              :items="PRIORITY_ITEMS"
              :selected-values="[priority]"
              :on-selection-change="(values: string[]) => (priority = values[0] as Priority)"
              :class="chipTrigger(false)"
            >
              <template #item-icon="{ item }">
                <Flag
                  class="size-4"
                  :style="{ color: PRIORITY_CONFIG[item.value as Priority].color }"
                  aria-hidden="true"
                />
              </template>
              <template #trigger>
                <span class="flex items-center gap-1.5">
                  <Flag
                    class="size-3.5 shrink-0"
                    :style="{ color: PRIORITY_CONFIG[priority].color }"
                    aria-hidden="true"
                  />
                  {{ PRIORITY_CONFIG[priority].label }}
                </span>
              </template>
            </Picker>
            <input type="hidden" name="priority" :value="priority" />

            <div
              v-if="membersLoading || membersError || members?.length"
              role="group"
              aria-labelledby="task-assignees-label"
              class="flex shrink-0 items-center gap-2"
            >
              <span id="task-assignees-label" class="sr-only">担当者</span>
              <p v-if="membersLoading" class="text-xs text-muted-foreground">
                メンバーを読み込み中...
              </p>
              <div v-else-if="membersError" class="flex items-center gap-2">
                <p role="alert" class="text-xs text-destructive">メンバーの取得に失敗しました</p>
                <Button
                  type="button"
                  variant="tertiary"
                  label="再試行"
                  @click="emit('retryMembers')"
                />
              </div>
              <UserSelector
                v-else
                v-model="selectedAssigneeIds"
                top-layer
                :icon="null"
                label="担当者"
                :users="members ?? []"
                :current-user-id="currentUserId"
                search-label="メンバーを検索"
                empty-text="担当者に指定できる利用者がいません"
                no-match-text="一致するメンバーがいません"
                :class="chipTrigger(!selectedAssignees.length)"
              >
                <template #trigger>
                  <span class="flex items-center gap-1.5">
                    <template v-if="selectedAssignees.length">
                      <UserAvatar
                        :user="{
                          id: selectedAssignees[0].id,
                          username: selectedAssignees[0].username,
                          avatar_url: selectedAssignees[0].avatar_url,
                        }"
                        size="xs"
                        decorative
                      />
                      {{ selectedAssignees[0].username
                      }}{{
                        selectedAssignees.length > 1 ? ` +${selectedAssignees.length - 1}` : ''
                      }}
                    </template>
                    <template v-else>
                      <User class="size-3.5 shrink-0" aria-hidden="true" />
                      担当者
                    </template>
                  </span>
                </template>
              </UserSelector>
            </div>

            <div
              v-if="labelsLoading || labelsError || labels?.length"
              role="group"
              aria-labelledby="task-labels-label"
              class="flex shrink-0 items-center gap-2"
            >
              <span id="task-labels-label" class="sr-only">ラベル</span>
              <p v-if="labelsLoading" class="text-xs text-muted-foreground">
                ラベルを読み込み中...
              </p>
              <div v-else-if="labelsError" class="flex items-center gap-2">
                <p role="alert" class="text-xs text-destructive">ラベルの取得に失敗しました</p>
                <Button
                  type="button"
                  variant="tertiary"
                  label="再試行"
                  @click="emit('retryLabels')"
                />
              </div>
              <Dropdown
                v-else
                top-layer
                :icon="null"
                :label="`ラベル ${selectedLabels.map((label) => label.name).join(' ')}`.trim()"
                :items="labelItems"
                :on-select="toggleLabel"
                :class="chipTrigger(!selectedLabels.length)"
              >
                <template #trigger>
                  <span id="task-labels-value" class="flex items-center gap-1.5">
                    <template v-if="selectedLabels.length">
                      <span
                        v-for="label in selectedLabels.slice(0, 3)"
                        :key="label.id"
                        class="inline-flex items-center gap-1"
                      >
                        <span
                          class="inline-block size-2 rounded-full"
                          :style="{ backgroundColor: label.color }"
                          aria-hidden="true"
                        />
                        {{ label.name }}
                      </span>
                      <span v-if="selectedLabels.length > 3">+{{ selectedLabels.length - 3 }}</span>
                    </template>
                    <template v-else>
                      <Tag class="size-3.5 shrink-0" aria-hidden="true" />
                      ラベル
                    </template>
                  </span>
                </template>
              </Dropdown>
            </div>

            <!--
              日付は type="date" の Input にしない。途中までの入力や、WebKit が空欄に出す今日の日付は
              入力済みに見えるのに value が空で、何も送られない。期限より後かは送信時に検証する
            -->
            <TaskDatesChip
              id="task-dates"
              v-model:planned-start-on="plannedStartOn"
              v-model:soft-deadline="softDeadline"
              v-model:hard-deadline="hardDeadline"
              :invalid="plannedStartAfterSoft"
            />

            <TaskNumberChip
              v-model="estimate"
              label="見積"
              ariaLabel="見積もり"
              name="estimated_minutes"
              unit="分"
              :icon="Timer"
              :min="1"
              :max="MAX_ESTIMATE_MINUTES"
            />
            <TaskNumberChip
              v-model="progress"
              label="進捗"
              ariaLabel="進捗率"
              name="progress_pct"
              unit="%"
              :icon="Percent"
              :min="0"
              :max="MAX_PROGRESS_PCT"
            />
          </div>
        </div>

        <div
          v-if="validationMessage || requestError || successMessage"
          class="shrink-0 px-4 pb-3 md:px-6"
        >
          <p v-if="validationMessage" role="alert" class="text-sm text-destructive">
            {{ validationMessage }}
          </p>
          <p v-if="requestError" role="alert" class="text-sm text-destructive">
            {{ requestError }}
          </p>
          <p v-if="successMessage" role="status" class="text-sm text-emerald-600">
            {{ successMessage }}
          </p>
        </div>

        <div class="flex shrink-0 items-center gap-2 border-t px-4 py-2.5 md:px-5 md:py-3">
          <span class="hidden text-xs text-muted-foreground md:inline">Esc で閉じる</span>
          <div class="ml-auto flex items-center gap-2">
            <Button
              type="button"
              variant="tertiary"
              :disabled="createMutation.isPending.value"
              label="取り消す"
              @click="onClose"
            />
            <Button
              type="submit"
              :disabled="createMutation.isPending.value || !isHydrated || !statusId"
              :label="createMutation.isPending.value ? '作成中...' : '作成'"
            >
              <template #icon>
                <Spinner v-if="createMutation.isPending.value" hide-label />
              </template>
            </Button>
          </div>
        </div>
      </HydrationSafeForm>
    </Dialog>
    <ConfirmDialog
      :open="discardConfirmOpen"
      title="入力を破棄しますか？"
      message="入力した内容は保存されていません。閉じると失われます。"
      confirm-label="破棄する"
      cancel-label="戻る"
      destructive
      :on-confirm="confirmDiscard"
      :on-cancel="cancelDiscard"
    />
  </div>
</template>

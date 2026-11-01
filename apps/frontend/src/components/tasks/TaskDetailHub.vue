<script setup lang="ts">
import {
  CalendarClock,
  CalendarDays,
  ArrowRight,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  CircleDashed,
  Copy,
  EllipsisVertical,
  Filter,
  Flag,
  Loader2,
  Pencil,
  Percent,
  Plus,
  Search,
  ListChecks,
  SquarePen,
  Star,
  Tag,
  Timer,
  User,
  UserPlus,
  X,
} from '@lucide/vue';
import { computed, nextTick, onBeforeUnmount, ref } from 'vue';
import type { ComponentPublicInstance } from 'vue';
import type { components } from '@/generated/api';
import type { EditableField } from '@/components/tasks/editable-field';
import TaskAssigneePicker from '@/components/tasks/TaskAssigneePicker.vue';
import TaskPropertyRow from '@/components/tasks/TaskPropertyRow.vue';
import {
  AvatarGroup,
  Button,
  Dropdown,
  Input,
  type DropdownItem,
  type DropdownSection,
} from '@koyori-app/ui-vue';
import MarkdownEditor from '@/components/markdown/MarkdownEditor.vue';
import { lookup, toPlainText } from '@/lib/comment-mention';
import {
  PRIORITY_CONFIG,
  clampProgressPct,
  formatDeadline,
  formatProgressPct,
  formatTaskDate,
  isoToLocalDateInput,
  taskSeqKey,
} from '@/lib/task-display';
// KFM サイドカー CSS の消費契約 (@/lib/markup-renderer/index.ts): v-html する消費側が
// 明示 import する。GFM は器の kfm-content (KFM_CONTENT_CLASS) が無いと一行も当たらない。
// content-class.ts は leaf module なので、この import で KFM コアが client へ載ることはない。
import { KFM_CONTENT_CLASS } from '@/lib/remark-gfm/content-class';
import '@/lib/remark-gfm/style.css';
import '@/lib/remark-koyori-alerts/style.css';
import '@/lib/rehype-starry-night/style.css';
import '@/lib/rehype-kfm-code/style.css';
import '@/lib/remark-kfm-mermaid/style.css';
import '@/lib/remark-kfm-mention/style.css';

type TaskDetail = components['schemas']['TaskDetailResponse'];
type StatusOption = components['schemas']['ProjectStatusResponse'];
type LabelOption = components['schemas']['LabelResponse'];

const props = defineProps<{
  task: TaskDetail | null;
  projectKey: string;
  statuses: StatusOption[];
  statusId: string;
  statusUpdating?: boolean;
  statusError?: string | null;
  priorityUpdating?: boolean;
  priorityError?: string | null;
  /** 担当者に選べるメンバー（未指定なら担当者は表示のみ） */
  members?: { id: string; username: string; avatar_url?: string | null }[];
  /** 担当者の更新中。ピッカーを止める（止めないと 2 回目が無言で捨てられる） */
  assigneeUpdating?: boolean;
  assigneeError?: string | null;
  /** 候補の取得状態。取得中・失敗を「候補 0 人」と混ぜない */
  membersState?: { loading?: boolean; error?: boolean; onRetry?: () => void };
  /** 担当者のプロフィール画面の URL。渡すと担当者名をリンクで並べる */
  profileHref?: (userId: string) => string;
  projectLabels?: LabelOption[];
  projectLabelsLoading?: boolean;
  projectLabelsError?: boolean;
  labelsUpdating?: boolean;
  labelsError?: string | null;
  fieldUpdating?: Partial<Record<EditableField, boolean>>;
  fieldErrors?: Partial<Record<EditableField, string>>;
  /** 直近に保存が確定したフィールド。該当箇所に「保存しました」を出す（失敗表示が優先） */
  savedField?: EditableField | null;
  loading?: boolean;
  notFound?: boolean;
  error?: boolean;
  deleteDisabled?: boolean;
  /**
   * 'page'（既定）はフルページ用に広画面で 3 カラムへ展開する。
   * 'pane' は分割ビューの狭い右ペイン用に常に 1 カラムで縦積みにする。
   */
  layout?: 'page' | 'pane';
  /**
   * サーバ (+data.ts) の renderDescription 出力。descriptionSource が最新の
   * task.description と厳密一致するときだけ KFM HTML として v-html 表示する。
   * null / 未指定・不一致はプレーンテキスト表示へフォールバックする
   * (分割ビューのペイン等、サーバ生成 HTML を持たない消費側)。
   * v-html に入れてよいのはこの prop だけ — task.description (生テキスト) を
   * v-html へ流す経路を作ってはならない (SSR/sanitize 契約: @/lib/markup-renderer)。
   */
  descriptionHtml?: string | null;
  /**
   * descriptionHtml の描画元テキスト (+data.ts の descriptionSource)。
   * descriptionHtml を渡す消費側は必ず対で渡す。task.description との厳密一致を
   * 下の freshDescriptionHtml が照合し、古い HTML (保存直後の reload 完了前・
   * reload 失敗・他者更新) が v-html に出る経路をコンポーネント側で塞ぐ。
   */
  descriptionSource?: string | null;
}>();

const emit = defineEmits<{
  'update:statusId': [value: string];
  'change:priority': [value: TaskDetail['priority']];
  'save:title': [value: string];
  'save:description': [value: string | null];
  'save:progress_pct': [value: number];
  'save:planned_start_on': [value: string | null];
  'save:soft_deadline': [value: string | null];
  'save:hard_deadline': [value: string | null];
  'save:label_ids': [value: string[]];
  'toggle:assignee': [userId: string, checked: boolean];
  'delete-request': [];
}>();

/** コピー結果を出しておく時間（ミリ秒） */
const COPY_RESET_MS = 2000;

/** タスク ID のコピー結果。非 secure context では clipboard が無いので失敗も示す */
const copyState = ref<'idle' | 'copied' | 'error'>('idle');
let copyResetTimer: ReturnType<typeof setTimeout> | undefined;
/** コピー操作の世代。連続して押したとき、最後の操作だけが表示とタイマーを持つ */
let copyRequest = 0;

function clearCopyReset() {
  if (copyResetTimer) {
    clearTimeout(copyResetTimer);
    copyResetTimer = undefined;
  }
}

async function copyTaskId(seqKey: string) {
  const request = ++copyRequest;
  // 実行中は前の結果を出したままにする（新しい操作の結果が出る前に消さない）
  clearCopyReset();

  let result: 'copied' | 'error';
  try {
    await navigator.clipboard.writeText(seqKey);
    result = 'copied';
  } catch {
    result = 'error';
  }

  // 遅れて返ってきた古い操作は、新しい操作の表示もタイマーも触らない
  if (request !== copyRequest) return;
  clearCopyReset();
  copyState.value = result;
  copyResetTimer = setTimeout(() => (copyState.value = 'idle'), COPY_RESET_MS);
}

onBeforeUnmount(clearCopyReset);

const resolvedStatus = computed(() =>
  props.statuses.find((status) => status.id === props.statusId),
);

const priorityOptions = Object.entries(PRIORITY_CONFIG) as [
  TaskDetail['priority'],
  (typeof PRIORITY_CONFIG)[TaskDetail['priority']],
][];

const TASK_ACTION_ITEMS: DropdownItem[] = [{ value: 'delete', label: '削除', destructive: true }];

const statusItems = computed<DropdownItem[]>(() =>
  props.statuses.map((status) => ({
    value: status.id,
    label: status.name,
    kind: 'radio',
    checked: status.id === props.statusId,
    disabled: props.statusUpdating,
  })),
);

const priorityItems = computed<DropdownItem[]>(() =>
  priorityOptions.map(([value, config]) => ({
    value,
    label: config.label,
    kind: 'radio',
    checked: value === props.task?.priority,
    disabled: props.priorityUpdating,
  })),
);

/** 「完了にする」の移動先。既定の完了に印が無ければ並び順で最初の完了へ倒す。 */
const doneStatus = computed(
  () =>
    props.statuses.find((status) => status.is_done_state && status.is_default_done) ??
    props.statuses.find((status) => status.is_done_state),
);

/** ワークフロー順に並べたステータス。「次へ」はこの並びで 1 つ進む。 */
const orderedStatuses = computed(() => [...props.statuses].sort((a, b) => a.position - b.position));

/** 現在の次のステータス。最後なら null（矢印は出さない）。 */
const nextStatus = computed(() => {
  const index = orderedStatuses.value.findIndex((status) => status.id === props.statusId);
  if (index < 0) return null;
  return orderedStatuses.value[index + 1] ?? null;
});

// 古い HTML の遮断: 描画元 (descriptionSource) がクライアントの最新 task.description と
// 厳密一致しない descriptionHtml は捨て、プレーンテキスト表示へ倒す。v-html の直前で
// 照合するのは、消費側の渡し忘れ・渡し間違いでも stale HTML が漏れないようにするため。
/**
 * KFM 表示の前（取得中・失敗・保存直後で HTML が古い間）に出すプレーン表示用の説明。
 * メンションの印（`<@利用者ID>`）は id を含むので、札と同じ字（`@今の名`）に直して出す。
 */
const plainDescription = computed(() => {
  const description = props.task?.description;
  if (!description) return '';
  const table = props.task?.mentions ?? {};
  return toPlainText(description, (id) => lookup(table, id));
});

const freshDescriptionHtml = computed(() => {
  if (!props.descriptionHtml || !props.task?.description) return null;
  if (props.descriptionSource !== props.task.description) return null;
  return props.descriptionHtml;
});
const editingField = ref<EditableField | null>(null);
const draftValue = ref('');
const editingControlRef = ref<HTMLElement | ComponentPublicInstance | null>(null);

function isFieldUpdating(field: EditableField) {
  return props.fieldUpdating?.[field] ?? false;
}

function fieldError(field: EditableField) {
  return props.fieldErrors?.[field] ?? null;
}

async function startEditing(field: EditableField) {
  if (!props.task || isFieldUpdating(field)) return;
  editingField.value = field;
  switch (field) {
    case 'title':
      draftValue.value = props.task.title;
      break;
    case 'description':
      draftValue.value = props.task.description ?? '';
      break;
    case 'progress_pct':
      draftValue.value = String(props.task.progress_pct);
      break;
    case 'planned_start_on':
      draftValue.value = props.task.planned_start_on ?? '';
      break;
    case 'soft_deadline':
      draftValue.value = isoToLocalDateInput(props.task.soft_deadline);
      break;
    case 'hard_deadline':
      draftValue.value = isoToLocalDateInput(props.task.hard_deadline);
      break;
  }
  await nextTick();
  const control = editingControlRef.value;
  // MarkdownEditor は器の div を focus しても効かない (実体は CodeMirror の
  // contenteditable)。expose された focus() を優先し、素の要素だけ DOM の focus へ落とす
  if (control && typeof (control as { focus?: unknown }).focus === 'function') {
    (control as { focus: () => void }).focus();
    return;
  }
  const element =
    control instanceof HTMLElement ? control : (control?.$el as HTMLElement | undefined);
  // koyori の Input の root は枠の span なので、中の input に当てる
  (element?.querySelector('input') ?? element)?.focus();
}

function cancelEditing() {
  editingField.value = null;
  draftValue.value = '';
}

function commitEditing(field: EditableField) {
  if (!props.task) return;
  // 編集を閉じた後に届く確定要求は捨てる。編集器の破棄で blur が飛ぶ経路があり、
  // これが無いと取り消し (Escape) の直後に空の下書きが確定として保存される
  if (editingField.value !== field) return;

  switch (field) {
    case 'title': {
      const next = draftValue.value.trim();
      if (!next) {
        cancelEditing();
        return;
      }
      if (next !== props.task.title) emit('save:title', next);
      break;
    }
    case 'description': {
      const trimmed = draftValue.value.trim();
      const current = props.task.description ?? '';
      if (trimmed === current) break;
      emit('save:description', trimmed.length ? trimmed : null);
      break;
    }
    case 'progress_pct': {
      const raw = String(draftValue.value);
      if (!raw.trim()) break;
      const parsed = Number(raw);
      if (!Number.isFinite(parsed)) break;
      const next = clampProgressPct(parsed);
      if (next !== props.task.progress_pct) emit('save:progress_pct', next);
      break;
    }
    case 'planned_start_on': {
      // Soft 期限より後なら送らずに編集を開いたまま警告を見せる（サーバーは検証しない）
      if (plannedStartAfterSoft.value) return;
      const next = draftValue.value.trim();
      if (next !== (props.task.planned_start_on ?? '')) emit('save:planned_start_on', next || null);
      break;
    }
    case 'soft_deadline': {
      const current = isoToLocalDateInput(props.task.soft_deadline);
      if (draftValue.value === current) break;
      emit('save:soft_deadline', draftValue.value.trim() ? draftValue.value.trim() : null);
      break;
    }
    case 'hard_deadline': {
      const current = isoToLocalDateInput(props.task.hard_deadline);
      if (draftValue.value === current) break;
      emit('save:hard_deadline', draftValue.value.trim() ? draftValue.value.trim() : null);
      break;
    }
  }

  cancelEditing();
}

function onEditKeydown(event: KeyboardEvent, field: EditableField) {
  if (event.key === 'Escape') {
    event.preventDefault();
    cancelEditing();
    return;
  }
  if (event.key === 'Enter' && field !== 'description') {
    event.preventDefault();
    commitEditing(field);
  }
}

function hasLabel(labelId: string) {
  return !!props.task?.labels.some((label) => label.id === labelId);
}

const labelSections = computed<DropdownSection[]>(() => [
  {
    heading: '選択してください',
    items: (props.projectLabels ?? []).map((label) => ({
      value: label.id,
      label: label.name,
      kind: 'checkbox',
      checked: hasLabel(label.id),
      disabled: props.labelsUpdating || props.projectLabelsError,
    })),
  },
]);

function toggleLabel(labelId: string, checked: boolean) {
  // 一覧の取得に失敗している間は、表示しているラベルが現在の集合か判断できない。
  // 引き算には使わない（下記）が、操作の受付自体は止める。UI 側もチェックボックスを
  // disabled にしてあり、押せるのに何も起きない状態にはならない
  if (!props.task || props.labelsUpdating || props.projectLabelsError) return;
  // projectLabels は独立キャッシュで task.labels より古いことがあるため、
  // 交差を取って「一覧に無い = 削除済み」と推定しない（有効なラベルを暗黙解除してしまう）。
  // 実際に削除済みのラベルが混ざって 400 になった場合は保存側でロールバックと再取得を行う
  const current = props.task.labels.map((label) => label.id);
  const next = checked ? [...current, labelId] : current.filter((id) => id !== labelId);
  emit('save:label_ids', next);
}

/** モックにある導線のうち、まだ対応していないもの。disabled で形だけ置く。 */
const PENDING_ACTIONS = [
  { label: 'フィールドを追加', icon: SquarePen },
  { label: 'チェックリストを作成', icon: ListChecks },
];

const plannedStartAfterSoft = computed(() => {
  if (editingField.value !== 'planned_start_on' || !props.task?.soft_deadline) return false;
  const draft = draftValue.value.trim();
  return !!draft && draft > isoToLocalDateInput(props.task.soft_deadline);
});

function clearDeadline(field: 'planned_start_on' | 'soft_deadline' | 'hard_deadline') {
  if (isFieldUpdating(field)) return;
  if (field === 'planned_start_on') emit('save:planned_start_on', null);
  else if (field === 'soft_deadline') emit('save:soft_deadline', null);
  else emit('save:hard_deadline', null);
  cancelEditing();
}
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col" data-task-detail-hub>
    <div v-if="loading" class="flex justify-center py-16">
      <Loader2 class="h-8 w-8 animate-spin text-muted-foreground" />
    </div>

    <div
      v-else-if="error"
      class="rounded-lg border border-destructive/30 bg-destructive/5 p-6 text-sm text-destructive"
    >
      タスクの読み込みに失敗しました
    </div>

    <div v-else-if="notFound" class="rounded-lg border p-6 text-sm text-muted-foreground">
      タスクが見つかりません
    </div>

    <template v-else-if="task">
      <!-- 上部バー: パンくず（消費側）／タスク ID ／操作 -->
      <!--
        参照どおり、最上部から左右に割る。上部バーを全幅にすると
        真ん中の縦線が上部バーの下からしか引けず、列が途中から始まって見える。
      -->
      <!--
        スクロールする器はこの div。左右 2 列に割るのは lg 以上だけで、そこでは
        本文と履歴がそれぞれ内側でスクロールする（親は lg:overflow-hidden で止める）。
        ペイン表示と lg 未満は 1 列に積むので、ここが唯一のスクロール容器になる。
        親（SplitPane のペイン / オーバーレイの Dialog）は高さを固定するため、
        ここに overflow を持たせないと、はみ出した分に到達できなくなる。
      -->
      <div
        class="flex min-h-0 flex-1 flex-col overflow-y-auto"
        :class="layout === 'pane' ? 'gap-6 px-4 py-4' : 'lg:flex-row lg:overflow-hidden'"
      >
        <div class="flex min-w-0 flex-col" :class="layout === 'pane' ? '' : 'lg:min-h-0 lg:flex-1'">
          <!--
          上部バー（モック）。前後移動・スター・Share は対応する機能がまだ無いので
          disabled で置く。押せるのに何も起きない状態にはしない。
        -->
          <header class="flex h-12 shrink-0 items-center gap-1 border-b px-3 pr-12">
            <Button type="button" variant="ghost" ariaLabel="前のタスク" disabled>
              <template #icon><ChevronUp class="size-4" aria-hidden="true" /></template>
            </Button>
            <Button type="button" variant="ghost" ariaLabel="次のタスク" disabled>
              <template #icon><ChevronDown class="size-4" aria-hidden="true" /></template>
            </Button>

            <div class="ml-auto flex items-center gap-1">
              <Button
                type="button"
                variant="ghost"
                class="mr-1"
                :label="taskSeqKey(projectKey, task.seq_id)"
                :ariaLabel="
                  copyState === 'copied'
                    ? 'タスクIDをコピーしました'
                    : copyState === 'error'
                      ? 'タスクIDをコピーできませんでした'
                      : `タスクID ${taskSeqKey(projectKey, task.seq_id)} をコピー`
                "
                @click="copyTaskId(taskSeqKey(projectKey, task.seq_id))"
              >
                <template #icon>
                  <Check v-if="copyState === 'copied'" class="size-3.5" aria-hidden="true" />
                  <X v-else-if="copyState === 'error'" class="size-3.5" aria-hidden="true" />
                  <Copy v-else class="size-3.5" aria-hidden="true" />
                </template>
              </Button>
              <Button type="button" variant="ghost" ariaLabel="スター" disabled>
                <template #icon><Star class="size-4" aria-hidden="true" /></template>
              </Button>
              <Button type="button" variant="tertiary" disabled label="共有">
                <template #icon><UserPlus class="size-4" aria-hidden="true" /></template>
              </Button>
              <Dropdown
                top-layer
                variant="ghost"
                ariaLabel="タスク操作"
                :disabled="deleteDisabled"
                :items="TASK_ACTION_ITEMS"
                :on-select="() => emit('delete-request')"
              >
                <template #icon><EllipsisVertical class="size-4" aria-hidden="true" /></template>
              </Dropdown>
              <slot name="header-actions" />
            </div>
          </header>

          <main
            class="flex min-w-0 flex-col gap-5"
            :class="
              layout === 'pane' ? '' : 'lg:min-h-0 lg:flex-1 lg:overflow-y-auto lg:px-6 lg:py-5'
            "
          >
            <!-- 種別。タスク種別の概念がまだ無いので固定表示（モックの Task チップ） -->
            <span
              class="inline-flex h-7 w-fit items-center gap-1.5 rounded-md border px-2 text-xs text-muted-foreground"
            >
              <CircleDashed class="size-3.5" aria-hidden="true" />
              タスク
            </span>

            <div>
              <div v-if="editingField === 'title'" class="flex flex-col gap-1">
                <Input
                  ref="editingControlRef"
                  data-editing="title"
                  class="font-semibold [--koyori-font-size-sm:1.5rem]"
                  :disabled="isFieldUpdating('title')"
                  aria-label="タイトル"
                  :value="draftValue"
                  :on-value-change="(v: string) => (draftValue = v)"
                  :on-blur="() => commitEditing('title')"
                  @keydown="onEditKeydown($event, 'title')"
                />
              </div>
              <button
                v-else
                type="button"
                class="group -mx-1 flex w-full items-start gap-2 rounded px-1 py-1 text-left transition-colors hover:bg-muted/40"
                :disabled="isFieldUpdating('title')"
                @click="startEditing('title')"
              >
                <h1 class="text-2xl font-semibold tracking-tight">{{ task.title }}</h1>
                <Pencil
                  class="mt-1.5 size-4 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
                  aria-hidden="true"
                />
              </button>
              <p v-if="fieldError('title')" class="mt-1 text-xs text-destructive">
                {{ fieldError('title') }}
              </p>
              <p
                v-else-if="savedField === 'title'"
                role="status"
                class="mt-1 text-xs text-muted-foreground"
              >
                保存しました
              </p>
            </div>

            <!--
            プロパティ。参照どおり左右の列を固定する（DOM 順で折り返すと並びが崩れる）。
            値は枠で囲わず、押せるところだけホバーで反応させる。
          -->
            <div class="grid grid-cols-1 gap-x-12 sm:grid-cols-2">
              <div class="flex flex-col">
                <TaskPropertyRow label="ステータス" :icon="CircleDashed">
                  <div class="flex items-center gap-1">
                    <!--
                      ピルは 2 つのボタンでできている。名前を押すと一覧から選ぶ、
                      右の矢印はワークフロー順の次へ 1 つ進める（入れ子のボタンは作れないので
                      見た目だけ 1 つの器に見せる）。
                    -->
                    <div class="inline-flex h-7 max-w-full items-center rounded bg-muted">
                      <!-- 一覧は根の中に出るので、器で切り取らない（overflow-hidden を付けない） -->
                      <Dropdown
                        top-layer
                        class="min-w-0"
                        label="ステータス"
                        :disabled="statusUpdating"
                        :icon="null"
                        :items="statusItems"
                        :on-select="(id: string) => emit('update:statusId', id)"
                      >
                        <template #trigger>
                          <span class="truncate text-xs font-semibold uppercase tracking-wide">{{
                            resolvedStatus?.name ?? '未設定'
                          }}</span>
                        </template>
                      </Dropdown>

                      <button
                        v-if="nextStatus"
                        type="button"
                        class="inline-flex h-7 items-center rounded-r border-l border-background/60 px-1.5 hover:bg-muted-foreground/10 disabled:opacity-50"
                        :aria-label="`${nextStatus.name} にする`"
                        :title="`次へ: ${nextStatus.name}`"
                        :disabled="statusUpdating"
                        @click="emit('update:statusId', nextStatus.id)"
                      >
                        <ChevronRight class="size-3.5 opacity-70" aria-hidden="true" />
                      </button>
                    </div>

                    <!-- ✓ は完了扱いのステータスへ一手で移す -->
                    <Button
                      v-if="doneStatus && doneStatus.id !== statusId"
                      type="button"
                      variant="ghost"
                      :ariaLabel="`${doneStatus.name} にする（完了）`"
                      :disabled="statusUpdating"
                      @click="emit('update:statusId', doneStatus.id)"
                    >
                      <template #icon><Check class="size-4" aria-hidden="true" /></template>
                    </Button>
                  </div>
                  <p v-if="statusError" class="mt-1 text-xs text-destructive">{{ statusError }}</p>
                </TaskPropertyRow>

                <!-- 日付は 1 行に 2 つ（参照の Start → Due） -->
                <TaskPropertyRow label="日付" :icon="CalendarDays">
                  <div class="flex flex-wrap items-center gap-1">
                    <div v-if="editingField === 'planned_start_on'" class="flex items-center gap-1">
                      <!-- koyori の Input は date に max / aria-invalid を付けない。期限より後かは保存時に止め、下の alert で知らせる -->
                      <Input
                        ref="editingControlRef"
                        data-editing="planned_start_on"
                        type="date"
                        class="max-w-40"
                        :disabled="isFieldUpdating('planned_start_on')"
                        aria-label="開始予定日"
                        :value="draftValue"
                        :on-value-change="(v: string) => (draftValue = v)"
                        :on-blur="() => commitEditing('planned_start_on')"
                        @keydown="onEditKeydown($event, 'planned_start_on')"
                      />
                      <Button
                        type="button"
                        variant="ghost"
                        class="shrink-0"
                        ariaLabel="開始予定日を消す"
                        :disabled="isFieldUpdating('planned_start_on')"
                        @mousedown.prevent
                        @click="clearDeadline('planned_start_on')"
                      >
                        <template #icon><X class="size-4" aria-hidden="true" /></template>
                      </Button>
                    </div>
                    <button
                      v-else
                      type="button"
                      class="inline-flex h-7 items-center gap-1.5 rounded px-1.5 text-sm hover:bg-muted"
                      :class="task.planned_start_on ? '' : 'text-muted-foreground'"
                      aria-label="開始予定日を編集"
                      :disabled="isFieldUpdating('planned_start_on')"
                      @click="startEditing('planned_start_on')"
                    >
                      <CalendarDays class="size-4" aria-hidden="true" />
                      {{ task.planned_start_on ?? '開始予定' }}
                    </button>

                    <ArrowRight class="size-3.5 text-muted-foreground" aria-hidden="true" />

                    <div v-if="editingField === 'soft_deadline'" class="flex items-center gap-1">
                      <Input
                        ref="editingControlRef"
                        data-editing="soft_deadline"
                        type="date"
                        class="max-w-40"
                        :disabled="isFieldUpdating('soft_deadline')"
                        aria-label="ソフト期限"
                        :value="draftValue"
                        :on-value-change="(v: string) => (draftValue = v)"
                        :on-blur="() => commitEditing('soft_deadline')"
                        @keydown="onEditKeydown($event, 'soft_deadline')"
                      />
                      <Button
                        type="button"
                        variant="ghost"
                        class="shrink-0"
                        ariaLabel="ソフト期限を消す"
                        :disabled="isFieldUpdating('soft_deadline')"
                        @mousedown.prevent
                        @click="clearDeadline('soft_deadline')"
                      >
                        <template #icon><X class="size-4" aria-hidden="true" /></template>
                      </Button>
                    </div>
                    <button
                      v-else
                      type="button"
                      class="inline-flex h-7 items-center gap-1.5 rounded px-1.5 text-sm hover:bg-muted"
                      :class="
                        task.soft_deadline
                          ? formatDeadline(task.soft_deadline, {
                              done: resolvedStatus?.is_done_state,
                            })?.overdue
                            ? 'text-destructive'
                            : ''
                          : 'text-muted-foreground'
                      "
                      aria-label="ソフト期限を編集"
                      :disabled="isFieldUpdating('soft_deadline')"
                      @click="startEditing('soft_deadline')"
                    >
                      <CalendarDays class="size-4" aria-hidden="true" />
                      {{
                        formatDeadline(task.soft_deadline, {
                          done: resolvedStatus?.is_done_state,
                          completedAt: task.completed_at,
                        })?.label ?? '期限'
                      }}
                    </button>

                    <ArrowRight class="size-3.5 text-muted-foreground" aria-hidden="true" />

                    <div v-if="editingField === 'hard_deadline'" class="flex items-center gap-1">
                      <Input
                        ref="editingControlRef"
                        data-editing="hard_deadline"
                        type="date"
                        class="max-w-40"
                        :disabled="isFieldUpdating('hard_deadline')"
                        aria-label="ハード期限"
                        :value="draftValue"
                        :on-value-change="(v: string) => (draftValue = v)"
                        :on-blur="() => commitEditing('hard_deadline')"
                        @keydown="onEditKeydown($event, 'hard_deadline')"
                      />
                      <Button
                        type="button"
                        variant="ghost"
                        class="shrink-0"
                        ariaLabel="ハード期限を消す"
                        :disabled="isFieldUpdating('hard_deadline')"
                        @mousedown.prevent
                        @click="clearDeadline('hard_deadline')"
                      >
                        <template #icon><X class="size-4" aria-hidden="true" /></template>
                      </Button>
                    </div>
                    <button
                      v-else
                      type="button"
                      class="inline-flex h-7 items-center gap-1.5 rounded px-1.5 text-sm hover:bg-muted"
                      :class="
                        task.hard_deadline
                          ? formatDeadline(task.hard_deadline, {
                              done: resolvedStatus?.is_done_state,
                            })?.overdue
                            ? 'text-destructive'
                            : ''
                          : 'text-muted-foreground'
                      "
                      aria-label="ハード期限を編集"
                      :disabled="isFieldUpdating('hard_deadline')"
                      @click="startEditing('hard_deadline')"
                    >
                      <CalendarClock class="size-4" aria-hidden="true" />
                      {{
                        formatDeadline(task.hard_deadline, {
                          done: resolvedStatus?.is_done_state,
                          completedAt: task.completed_at,
                        })?.label ?? 'ハード期限'
                      }}
                    </button>
                  </div>
                  <p
                    v-if="plannedStartAfterSoft"
                    class="mt-1 text-xs text-destructive"
                    role="alert"
                  >
                    開始予定日が期限より後になっています
                  </p>
                  <p
                    v-else-if="
                      fieldError('planned_start_on') ||
                      fieldError('soft_deadline') ||
                      fieldError('hard_deadline')
                    "
                    class="mt-1 text-xs text-destructive"
                  >
                    {{
                      fieldError('planned_start_on') ??
                      fieldError('soft_deadline') ??
                      fieldError('hard_deadline')
                    }}
                  </p>
                  <p
                    v-else-if="
                      savedField === 'planned_start_on' ||
                      savedField === 'soft_deadline' ||
                      savedField === 'hard_deadline'
                    "
                    role="status"
                    class="mt-1 text-xs text-muted-foreground"
                  >
                    保存しました
                  </p>
                </TaskPropertyRow>

                <TaskPropertyRow
                  label="見積"
                  :icon="Timer"
                  :filled="task.estimated_minutes != null"
                  empty-text="未設定"
                >
                  <span v-if="task.estimated_minutes != null">{{ task.estimated_minutes }} 分</span>
                </TaskPropertyRow>

                <div data-task-labels>
                  <TaskPropertyRow label="ラベル" :icon="Tag">
                    <!--
                      値の欄そのものを押して開く（参照）。選択済みは欄の中にチップで並べ、
                      右の × でまとめて外す。個別に外すのはメニューの中から。
                    -->
                    <div class="flex min-h-8 items-center gap-1 rounded hover:bg-muted">
                      <!-- 更新中は項目だけを止める。Dropdown ごと disabled にすると一覧が閉じる -->
                      <Dropdown
                        top-layer
                        class="min-w-0 flex-1 [&>button]:w-full"
                        label="ラベルを編集"
                        :icon="null"
                        :disabled="projectLabelsLoading"
                        :items="labelSections"
                        :empty-message="
                          projectLabelsError ? 'ラベルを読み込めませんでした' : 'ラベルがありません'
                        "
                        :on-select="(id: string) => toggleLabel(id, !hasLabel(id))"
                      >
                        <template #trigger>
                          <span
                            class="flex min-w-0 flex-1 flex-wrap items-center gap-1.5 text-left"
                          >
                            <span v-if="!task.labels.length" class="text-muted-foreground">
                              未設定
                            </span>
                            <span
                              v-for="label in task.labels"
                              :key="label.id"
                              class="inline-flex items-center gap-1.5 rounded bg-muted px-2 py-0.5 text-xs"
                            >
                              <span
                                class="inline-block size-2 rounded-full"
                                :style="{ backgroundColor: label.color }"
                                aria-hidden="true"
                              />
                              {{ label.name }}
                            </span>
                          </span>
                        </template>
                      </Dropdown>

                      <Button
                        v-if="task.labels.length"
                        type="button"
                        variant="ghost"
                        class="shrink-0"
                        ariaLabel="ラベルをすべて外す"
                        :disabled="labelsUpdating"
                        @click="emit('save:label_ids', [])"
                      >
                        <template #icon><X class="size-4" aria-hidden="true" /></template>
                      </Button>
                    </div>
                    <p v-if="labelsError" class="mt-1 text-xs text-destructive">
                      {{ labelsError }}
                    </p>
                  </TaskPropertyRow>
                </div>
              </div>

              <div class="flex flex-col">
                <TaskPropertyRow
                  label="担当者"
                  :icon="User"
                  :filled="members ? true : task.assignees.length > 0"
                  empty-text="未設定"
                >
                  <!-- メンバーを渡された消費側では選び直せる。渡されなければ表示だけ -->
                  <TaskAssigneePicker
                    top-layer
                    v-if="members"
                    :members="members"
                    :selected="task.assignees.map((assignee) => assignee.user)"
                    :disabled="assigneeUpdating"
                    :members-state="membersState"
                    @toggle="(userId, checked) => emit('toggle:assignee', userId, checked)"
                  />
                  <AvatarGroup
                    v-else-if="task.assignees.length"
                    label="担当者"
                    :items="
                      task.assignees.map(({ user }) => ({
                        name: user.username,
                        src: user.avatar_url ?? undefined,
                      }))
                    "
                    :max="3"
                    :size="28"
                  />
                  <!-- ピッカーのトリガーはボタンなので、リンクは中に入れず横に並べる -->
                  <div
                    v-if="profileHref && task.assignees.length"
                    class="mt-1 flex flex-wrap gap-x-2 text-xs"
                    data-assignee-profile-links
                  >
                    <a
                      v-for="assignee in task.assignees"
                      :key="assignee.user.id"
                      :href="profileHref(assignee.user.id)"
                      class="text-muted-foreground hover:text-foreground hover:underline"
                    >
                      {{ assignee.user.username }}
                    </a>
                  </div>
                  <p v-if="assigneeError" class="mt-1 text-xs text-destructive">
                    {{ assigneeError }}
                  </p>
                </TaskPropertyRow>

                <TaskPropertyRow label="優先度" :icon="Flag">
                  <!--
                    ステータスと同じピル + メニューで変える（#665 の native select から
                    参照デザインへ載せ替え）。旗と文字色は優先度ごとの色で出す。
                  -->
                  <Dropdown
                    top-layer
                    class="[&>button]:text-(--priority-color)"
                    :style="{ '--priority-color': PRIORITY_CONFIG[task.priority].color }"
                    label="優先度"
                    :icon="null"
                    :disabled="priorityUpdating"
                    :items="priorityItems"
                    :on-select="
                      (value: string) => emit('change:priority', value as TaskDetail['priority'])
                    "
                  >
                    <template #trigger>
                      <span class="flex items-center gap-1.5">
                        <Flag class="size-4" aria-hidden="true" />
                        <span class="truncate">{{ PRIORITY_CONFIG[task.priority].label }}</span>
                      </span>
                    </template>
                  </Dropdown>
                  <p v-if="priorityError" class="mt-1 text-xs text-destructive">
                    {{ priorityError }}
                  </p>
                </TaskPropertyRow>

                <TaskPropertyRow label="進捗" :icon="Percent">
                  <Input
                    v-if="editingField === 'progress_pct'"
                    ref="editingControlRef"
                    data-editing="progress_pct"
                    type="number"
                    :min="0"
                    :max="100"
                    class="max-w-24"
                    :disabled="isFieldUpdating('progress_pct')"
                    aria-label="進捗率"
                    :value="draftValue"
                    :on-value-change="(v: string) => (draftValue = v)"
                    :on-blur="() => commitEditing('progress_pct')"
                    @keydown="onEditKeydown($event, 'progress_pct')"
                  />
                  <button
                    v-else
                    type="button"
                    class="inline-flex h-7 items-center rounded px-1.5 hover:bg-muted"
                    :disabled="isFieldUpdating('progress_pct')"
                    @click="startEditing('progress_pct')"
                  >
                    {{ formatProgressPct(task.progress_pct) }}
                  </button>
                  <p v-if="fieldError('progress_pct')" class="mt-1 text-xs text-destructive">
                    {{ fieldError('progress_pct') }}
                  </p>
                  <p
                    v-else-if="savedField === 'progress_pct'"
                    role="status"
                    class="mt-1 text-xs text-muted-foreground"
                  >
                    保存しました
                  </p>
                </TaskPropertyRow>
              </div>
            </div>

            <!-- 説明 -->
            <div class="border-t pt-5">
              <div class="mb-2 flex items-center justify-between gap-2">
                <h2 class="text-sm font-medium text-muted-foreground">説明</h2>
                <Button
                  v-if="editingField === 'description' && task.description"
                  type="button"
                  variant="ghost"
                  :disabled="isFieldUpdating('description')"
                  label="クリア"
                  @mousedown.prevent
                  @click="
                    emit('save:description', null);
                    cancelEditing();
                  "
                />
                <Button
                  v-else-if="editingField !== 'description' && freshDescriptionHtml"
                  type="button"
                  variant="ghost"
                  ariaLabel="説明を編集"
                  :disabled="isFieldUpdating('description')"
                  @click="startEditing('description')"
                >
                  <template #icon><Pencil class="size-4" aria-hidden="true" /></template>
                </Button>
              </div>

              <MarkdownEditor
                v-if="editingField === 'description'"
                v-model="draftValue"
                ref="editingControlRef"
                data-editing="description"
                :disabled="isFieldUpdating('description')"
                aria-label="説明"
                placeholder="markdown で書けます"
                :candidates="members ?? []"
                :mentions="task.mentions"
                @keydown="onEditKeydown($event, 'description')"
                @submit="commitEditing('description')"
                @blur="commitEditing('description')"
              />
              <!--
              KFM 表示は非対話の div にする: 描画 HTML はリンク等の対話要素を含みうるため、
              プレーン表示のような button で包むと入れ子の対話要素になる。編集導線は
              上の鉛筆ボタン。freshDescriptionHtml は task.description との厳密一致を
              照合済みで、説明が空・不一致 (stale) のときは null になり
              下のプレーン分岐へ倒れる。
            -->
              <div
                v-else-if="freshDescriptionHtml"
                :class="KFM_CONTENT_CLASS"
                class="text-sm leading-relaxed"
                data-task-description-html
                v-html="freshDescriptionHtml"
              />
              <button
                v-else
                type="button"
                class="group w-full rounded-md text-left transition-colors hover:bg-muted/40"
                :disabled="isFieldUpdating('description')"
                @click="startEditing('description')"
              >
                <p v-if="task.description" class="whitespace-pre-wrap text-sm leading-relaxed">
                  {{ plainDescription }}
                </p>
                <p v-else class="text-sm text-muted-foreground">説明を追加</p>
              </button>
              <p v-if="fieldError('description')" class="mt-2 text-xs text-destructive">
                {{ fieldError('description') }}
              </p>
              <p
                v-else-if="savedField === 'description'"
                role="status"
                class="mt-2 text-xs text-muted-foreground"
              >
                保存しました
              </p>
            </div>

            <!-- モックのうち未対応のアクション。サブタスクは直後の main slot で実装済み -->
            <ul class="flex flex-col gap-1">
              <li v-for="action in PENDING_ACTIONS" :key="action.label">
                <Button type="button" variant="ghost" class="w-full" :label="action.label" disabled>
                  <template #icon>
                    <component :is="action.icon" class="size-4" aria-hidden="true" />
                  </template>
                </Button>
              </li>
            </ul>

            <slot name="main" />

            <p class="pb-2 text-xs text-muted-foreground">
              作成 {{ formatTaskDate(task.created_at) }} / 更新
              {{ formatTaskDate(task.updated_at) }}
            </p>
          </main>
        </div>

        <aside
          class="flex min-w-0 flex-col"
          :class="
            layout === 'pane'
              ? 'gap-3 border-t pt-4'
              : 'lg:min-h-0 lg:w-[19rem] lg:shrink-0 lg:border-l'
          "
        >
          <!-- 列の見出し。左の上部バーと高さを揃える（右端は器の閉じるボタン分を空ける） -->
          <div
            class="flex items-center gap-1"
            :class="layout === 'pane' ? '' : 'h-12 shrink-0 border-b px-4 pr-12'"
          >
            <h2 class="text-sm font-semibold">アクティビティ</h2>
            <div class="ml-auto flex items-center gap-0.5">
              <Button type="button" variant="ghost" ariaLabel="アクティビティを検索" disabled>
                <template #icon><Search class="size-4" aria-hidden="true" /></template>
              </Button>
              <Button type="button" variant="ghost" ariaLabel="アクティビティを絞り込む" disabled>
                <template #icon><Filter class="size-4" aria-hidden="true" /></template>
              </Button>
            </div>
          </div>

          <div
            class="flex min-h-0 flex-1 flex-col"
            :class="layout === 'pane' ? '' : 'lg:px-4 lg:pt-3'"
          >
            <slot name="sidebar" />
          </div>
        </aside>
      </div>

      <slot name="footer" />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import {
  PhCalendarBlank,
  PhCalendarPlus,
  PhCaretRight,
  PhChat,
  PhFlag,
  PhTag,
} from '@phosphor-icons/vue';

import { Button, Dropdown, Input, Textarea, type DropdownItem } from '@koyori-app/ui-vue';
import {
  formatDeadline,
  isoToLocalDateInput,
  localDateInputToIso,
  PRIORITY_CONFIG,
} from '@/lib/task-display';
import type { components } from '@/generated/api';
import type { TaskRowField } from '@/composables/useTaskRowMutations';
import { TASK_ROW_GRID } from '@/components/tasks/task-grouped-columns';
import TaskAssigneePicker from '@/components/tasks/TaskAssigneePicker.vue';

type TaskResponse = components['schemas']['TaskResponse'];
type LabelResponse = components['schemas']['LabelResponse'];
type StatusResponse = components['schemas']['ProjectStatusResponse'];
type ProjectMember = { id: string; username: string; avatar_url?: string | null };

const props = withDefaults(
  defineProps<{
    task: TaskResponse;
    statuses: StatusResponse[];
    projectLabels: LabelResponse[];
    members: ProjectMember[];
    /** 担当者候補の取得状態。取得中・失敗を「候補 0 人」と混ぜない */
    membersState?: { loading?: boolean; error?: boolean; onRetry?: () => void };
    /** 更新中の項目。飛行中は同じ行の操作を止める */
    pendingField?: TaskRowField;
    error?: string;
    commentPending?: boolean;
    /** 空白部分から選ばれた行。選択後だけ展開矢印を見せる。 */
    selected?: boolean;
    expanded?: boolean;
    showSubtaskToggle?: boolean;
    /** 一覧では 1 段だけネスト表示する。 */
    depth?: 0 | 1;
    /** コメントの追加。成功したときだけ下書きを捨てるので、成否を返してもらう */
    onComment: (body: string) => Promise<boolean>;
  }>(),
  {
    selected: false,
    expanded: false,
    showSubtaskToggle: true,
    depth: 0,
  },
);

const emit = defineEmits<{
  open: [taskId: string];
  select: [];
  'toggle:subtasks': [];
  'update:status': [statusId: string];
  'update:priority': [priority: TaskResponse['priority']];
  'update:softDeadline': [iso: string | null];
  'toggle:assignee': [userId: string, checked: boolean];
  'toggle:label': [labelId: string, checked: boolean];
}>();

// 表示（並び・ラベル・色・アイコン）はテーブル表示と同じ定義を使う。
// ここで別に持つと同じ優先度が 2 つの見た目で出る
const PRIORITIES = Object.keys(PRIORITY_CONFIG) as TaskResponse['priority'][];

const isBusy = computed(() => props.pendingField !== undefined);
// 完了の状態にある task は期限切れにしない（formatDeadline の注釈を参照）
const isDone = computed(
  () => props.statuses.find((status) => status.id === props.task.status_id)?.is_done_state ?? false,
);
const deadline = computed(() => formatDeadline(props.task.soft_deadline, { done: isDone.value }));
const assignees = computed(() => props.task.assignees.map((entry) => entry.user));
const statusColor = computed(
  () => props.statuses.find((status) => status.id === props.task.status_id)?.color ?? null,
);
const statusName = computed(
  () => props.statuses.find((status) => status.id === props.task.status_id)?.name ?? '—',
);

// ---- 期限のインライン編集 ----
const editingDeadline = ref(false);
const deadlineDraft = ref('');
const deadlineInputRef = ref<InstanceType<typeof Input> | null>(null);

async function startEditingDeadline() {
  if (isBusy.value) return;
  deadlineDraft.value = isoToLocalDateInput(props.task.soft_deadline);
  editingDeadline.value = true;
  await nextTick();
  // koyori の Input の root は枠の span なので、中の input に当てる
  (deadlineInputRef.value?.$el as HTMLElement | undefined)?.querySelector('input')?.focus();
}

function commitDeadline() {
  if (!editingDeadline.value) return;
  editingDeadline.value = false;
  const raw = deadlineDraft.value.trim();
  const current = isoToLocalDateInput(props.task.soft_deadline);
  if (raw === current) return;
  // 変換は詳細画面と同じヘルパーを使う（画面ごとに時刻の寄せ方が変わらないように）
  emit('update:softDeadline', raw ? localDateInputToIso(raw) : null);
}

// ---- コメントのその場追加 ----
const commentOpen = ref(false);
const commentDraft = ref('');
const commentInputRef = ref<InstanceType<typeof Textarea> | null>(null);

async function toggleComment() {
  commentOpen.value = !commentOpen.value;
  if (!commentOpen.value) return;
  await nextTick();
  (commentInputRef.value?.$el as HTMLTextAreaElement | undefined)?.focus();
}

async function submitComment() {
  const body = commentDraft.value.trim();
  if (!body || props.commentPending) return;
  // 失敗したら下書きを残したまま欄を開けておく。閉じてから失敗を知らせても
  // 本文が戻らず、投稿できたと誤解させる
  if (!(await props.onComment(body))) return;
  commentDraft.value = '';
  commentOpen.value = false;
}

// ---- メニュー ----
const statusItems = computed<DropdownItem[]>(() =>
  props.statuses.map((status) => ({
    value: status.id,
    label: status.name,
    kind: 'radio',
    checked: status.id === props.task.status_id,
  })),
);

function hasLabel(labelId: string) {
  return props.task.labels.some((label) => label.id === labelId);
}

const labelItems = computed<DropdownItem[]>(() =>
  props.projectLabels.map((label) => ({
    value: label.id,
    label: label.name,
    kind: 'checkbox',
    checked: hasLabel(label.id),
    disabled: isBusy.value,
  })),
);

const priorityItems = computed<DropdownItem[]>(() =>
  PRIORITIES.map((value) => ({
    value,
    label: PRIORITY_CONFIG[value].label,
    kind: 'radio',
    checked: value === props.task.priority,
  })),
);

/** 既存の入力・メニュー・タイトル操作を横取りせず、行の空白だけを選択に使う。 */
function selectFromBlank(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (
    target.closest(
      'button, a, input, textarea, select, [role="menuitem"], [role="checkbox"], [role="radio"], [contenteditable="true"]',
    )
  ) {
    return;
  }
  emit('select');
}

function selectFromKeyboard(event: KeyboardEvent) {
  if (event.target !== event.currentTarget) return;
  if (event.key !== 'Enter' && event.key !== ' ') return;
  event.preventDefault();
  emit('select');
}
</script>

<template>
  <div class="group border-b border-border/60" :data-state="selected ? 'selected' : undefined">
    <!--
      最後の行の下の線を消すかは、行を並べる側が決める（TaskSubtaskBranch が
      last:border-b-0 を渡す）。ここに置くと、親の形で :last-child が変わったとき
      一覧の群の最後の行まで線が消える（DnD の container で包んだときに起きた）。
      根は単一の要素に保つ。渡された class は根へ付くので、註を根の外へ出して根が
      複数のノードになると、class がどこへ付くかが Vue の版と build の設定に依る。
    -->
    <div
      :class="[
        TASK_ROW_GRID,
        'transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-inset',
        selected && 'bg-muted/55',
      ]"
      tabindex="0"
      :aria-label="`タスク「${task.title}」を選択`"
      @click="selectFromBlank"
      @focus="emit('select')"
      @keydown="selectFromKeyboard"
    >
      <!-- 名前。タイトルの変更は詳細だけ（ここでは開く導線のみ） -->
      <div
        class="flex min-w-0 items-center gap-2 px-2 py-1.5"
        :class="depth === 1 ? 'pl-4' : undefined"
      >
        <button
          v-if="showSubtaskToggle"
          type="button"
          class="grid size-6 shrink-0 place-items-center rounded transition-[background-color,opacity] duration-150 motion-reduce:transition-none"
          :class="
            selected || expanded
              ? 'bg-muted text-foreground opacity-100 hover:bg-muted-foreground/15'
              : 'pointer-events-none opacity-0'
          "
          :aria-label="expanded ? 'サブタスクを折りたたむ' : 'サブタスクを展開'"
          :aria-expanded="expanded"
          @click.stop="emit('toggle:subtasks')"
        >
          <PhCaretRight
            class="size-3.5 transition-transform duration-150 motion-reduce:transition-none"
            :class="expanded && 'rotate-90'"
            aria-hidden="true"
          />
        </button>
        <span v-else class="size-6 shrink-0" aria-hidden="true" />

        <!-- グループがステータスなので列は持たず、名前の左の丸から変える（参照デザイン） -->
        <Dropdown
          class="shrink-0"
          top-layer
          :ariaLabel="`ステータス: ${statusName}`"
          :title="statusName"
          :disabled="isBusy"
          :icon="null"
          :items="statusItems"
          :on-select="(id: string) => emit('update:status', id)"
        >
          <template #trigger>
            <span
              class="grid size-4 place-items-center rounded-full border-2"
              :style="statusColor ? { borderColor: statusColor } : undefined"
            >
              <span
                v-if="statusColor"
                class="block size-1.5 rounded-full"
                :style="{ backgroundColor: statusColor }"
                aria-hidden="true"
              />
            </span>
          </template>
        </Dropdown>

        <button
          type="button"
          class="min-w-0 truncate text-left text-sm hover:underline"
          @click="emit('open', task.id)"
        >
          {{ task.title }}
        </button>

        <span
          v-for="label in task.labels"
          :key="label.id"
          class="inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground"
        >
          <span
            class="inline-block size-2 rounded-full"
            :style="{ backgroundColor: label.color }"
            aria-hidden="true"
          />
          {{ label.name }}
        </span>

        <!--
          ラベルの付け外し。行にカーソルを合わせたときだけ出す。一覧は根の中に出るので、
          開いている間（フォーカスが中にある間）も根を見せておく。
          更新中は項目だけを止める。Dropdown ごと disabled にすると一覧が閉じ、続けて選べない
        -->
        <Dropdown
          variant="ghost"
          top-layer
          class="shrink-0 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100"
          ariaLabel="ラベルを編集"
          title="ラベル"
          :items="labelItems"
          empty-message="ラベルがありません"
          :on-select="(id: string) => emit('toggle:label', id, !hasLabel(id))"
        >
          <template #icon><PhTag class="size-4" /></template>
        </Dropdown>
      </div>

      <!-- 担当者 -->
      <div class="px-2">
        <TaskAssigneePicker
          top-layer
          :members="members"
          :selected="assignees"
          :disabled="isBusy"
          :members-state="membersState"
          @toggle="(userId, checked) => emit('toggle:assignee', userId, checked)"
        />
      </div>

      <!-- 期限 -->
      <div class="px-2 text-sm">
        <Input
          v-if="editingDeadline"
          ref="deadlineInputRef"
          type="date"
          aria-label="期限"
          :value="deadlineDraft"
          :on-value-change="(v: string) => (deadlineDraft = v)"
          :on-blur="commitDeadline"
          @keydown.enter.prevent="commitDeadline"
          @keydown.esc.prevent="editingDeadline = false"
        />
        <button
          v-else-if="deadline"
          type="button"
          class="inline-flex items-center gap-1.5 rounded px-1 py-0.5 hover:bg-muted"
          :class="deadline.overdue ? 'text-red-500' : 'text-muted-foreground'"
          :disabled="isBusy"
          @click="startEditingDeadline"
        >
          <PhCalendarBlank class="size-4" />
          {{ deadline.label }}
        </button>
        <Button
          v-else
          type="button"
          variant="ghost"
          :disabled="isBusy"
          ariaLabel="期限を設定"
          @click="startEditingDeadline"
        >
          <template #icon><PhCalendarPlus class="size-4" /></template>
        </Button>
      </div>

      <!-- 優先度 -->
      <div class="px-2">
        <!-- アイコンと文字の両方に優先度の色を載せる（モックと同じ） -->
        <Dropdown
          variant="ghost"
          top-layer
          class="[&>button]:text-(--priority-color)"
          :style="{ '--priority-color': PRIORITY_CONFIG[task.priority].color }"
          :disabled="isBusy"
          ariaLabel="優先度"
          :label="PRIORITY_CONFIG[task.priority].label"
          :items="priorityItems"
          :on-select="(value: string) => emit('update:priority', value as TaskResponse['priority'])"
        >
          <template #icon><PhFlag class="size-4" aria-hidden="true" /></template>
        </Dropdown>
      </div>

      <!-- コメント -->
      <div class="px-2">
        <Button type="button" variant="ghost" ariaLabel="コメントを追加" @click="toggleComment">
          <template #icon><PhChat class="size-4" /></template>
        </Button>
      </div>
    </div>

    <div v-if="commentOpen" class="flex items-start gap-2 border-t bg-muted/20 px-3 py-2">
      <Textarea
        ref="commentInputRef"
        :rows="2"
        class="flex-1"
        placeholder="コメントを追加"
        aria-label="コメント"
        :disabled="commentPending"
        :value="commentDraft"
        :on-value-change="(v: string) => (commentDraft = v)"
        @keydown.enter.meta.prevent="submitComment"
        @keydown.enter.ctrl.prevent="submitComment"
        @keydown.esc.prevent="commentOpen = false"
      />
      <div class="flex flex-col gap-1">
        <Button
          type="button"
          :disabled="commentPending || !commentDraft.trim()"
          label="送信"
          @click="submitComment"
        />
        <Button type="button" variant="ghost" label="閉じる" @click="commentOpen = false" />
      </div>
    </div>

    <p v-if="error" class="px-3 pb-1.5 text-xs text-destructive">{{ error }}</p>
  </div>
</template>

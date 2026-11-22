<script setup lang="ts">
import {
  PhArrowCounterClockwise,
  PhArrowRight,
  PhCheck,
  PhClock,
  PhInfo,
  PhX,
} from '@phosphor-icons/vue';
import { computed, ref, watch, type Component } from 'vue';
import {
  Alert,
  Badge,
  Button,
  Dropdown,
  EllipsisIcon,
  Field,
  Textarea,
  type DropdownItem,
} from '@koyori-app/ui-vue';

import ReviewDeferredTaskLink from '@/components/reviews/ReviewDeferredTaskLink.vue';
import ReviewFindingBody from '@/components/reviews/ReviewFindingBody.vue';
import {
  SEVERITY_LABELS,
  STATE_LABELS,
  countsAsUnresolved,
  findingActionButtons,
  type FindingActionButton,
  type FindingState,
  type ReviewFinding,
} from '@/lib/review-findings';
import { relativeTime } from '@/lib/task-activity';

const NOTE_MAX_LENGTH = 2000;

const props = defineProps<{
  finding: ReviewFinding;
  /** 選んだ行だけを開く */
  selected: boolean;
  /** この指摘へのリンク（pathname + search） */
  href: string;
  /** 現在の利用者。自分の修正を自分で確認できない旨の説明を出すのに使う */
  viewerId: string;
  tenantId: string;
  tenantSlug: string;
  projectId: string;
  projectKey: string;
  /** 状態の遷移を待っている間（どの行の遷移でも） */
  pending: boolean;
  /** この行の遷移の失敗の文 */
  error: string | null;
}>();

const emit = defineEmits<{
  toggle: [];
  transition: [to: FindingState, note: string | null];
}>();

const ICONS: Record<FindingActionButton['icon'], Component> = {
  start: PhArrowRight,
  check: PhCheck,
  undo: PhArrowCounterClockwise,
  defer: PhClock,
  reject: PhX,
};

const FORMS = {
  defer: {
    to: 'deferred',
    title: 'この指摘を繰り延べる',
    description: '繰り延べると未解決に数えず、マージの判定から外れます。',
    placeholder: '今回は触らず、別の PR で扱う',
    required: false,
    confirm: '繰り延べる',
    icon: PhClock,
    variant: 'primary',
  },
  reject: {
    to: 'rejected',
    title: 'この指摘を取り下げる',
    description: '指摘が誤りだった、または対応しないと決めたときに使います。',
    placeholder: 'なぜ対応しないかを書く',
    required: true,
    confirm: '取り下げる',
    icon: PhX,
    variant: 'danger',
  },
} as const;

const actions = computed(() => findingActionButtons(props.finding, props.viewerId));
const unresolved = computed(() => countsAsUnresolved(props.finding.state));
const blocksMerge = computed(
  () => props.finding.severity === 'high' || props.finding.severity === 'medium',
);

/** 開いている理由の入力欄。null は操作のボタンを出している */
const openForm = ref<'defer' | 'reject' | null>(null);
const note = ref('');
const showHistory = ref(false);
const copied = ref(false);

const form = computed(() => (openForm.value ? FORMS[openForm.value] : null));
const trimmedNote = computed(() => note.value.trim());
const canConfirm = computed(
  () => form.value !== null && !props.pending && (!form.value.required || trimmedNote.value !== ''),
);

function closeForm() {
  openForm.value = null;
  note.value = '';
}

// 行を畳んだら、開いていた入力欄と履歴も畳む
watch(
  () => props.selected,
  (selected) => {
    if (selected) return;
    closeForm();
    showHistory.value = false;
  },
);
// 遷移が通って状態が変わったら、入力欄を閉じる（失敗したときは理由を残して開いたままにする）
watch(() => props.finding.state, closeForm);

function onTitleClick(event: MouseEvent) {
  // 修飾キーつきのクリックは、リンクとして別のタブで開かせる
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  event.preventDefault();
  emit('toggle');
}

function onAction(action: FindingActionButton) {
  if (action.form) {
    openForm.value = action.form;
    note.value = '';
    return;
  }
  emit('transition', action.to, null);
}

function confirmForm() {
  if (!form.value || !canConfirm.value) return;
  emit('transition', form.value.to, trimmedNote.value === '' ? null : trimmedNote.value);
}

const menuItems = computed<DropdownItem[]>(() => [
  { value: 'copy', label: 'リンクをコピー' },
  { value: 'history', label: showHistory.value && props.selected ? '履歴を隠す' : '履歴を表示' },
]);

async function onMenuSelect(value: string) {
  if (value === 'copy') {
    try {
      await navigator.clipboard.writeText(new URL(props.href, window.location.origin).href);
      copied.value = true;
      window.setTimeout(() => (copied.value = false), 2000);
    } catch {
      copied.value = false;
    }
    return;
  }
  if (!props.selected) {
    // 履歴は開いた行に出すので、選んでいない行は選んでから出す
    showHistory.value = true;
    emit('toggle');
    return;
  }
  showHistory.value = !showHistory.value;
}

/** 状態を変えた最後の 1 件（登録の行は from_state が無い）。 */
const lastTransition = computed(() =>
  props.finding.transitions.findLast((history) => history.from_state !== null),
);

function transitionText(history: ReviewFinding['transitions'][number]): string {
  return history.from_state
    ? `${history.actor.username} が ${STATE_LABELS[history.from_state]} → ${STATE_LABELS[history.to_state]} にしました`
    : `${history.actor.username} が ${STATE_LABELS[history.to_state]} として登録しました`;
}
</script>

<template>
  <li
    :id="`finding-${finding.id}`"
    class="border-b last:border-b-0 data-selected:[--muted-foreground:var(--koyori-color-text-secondary)] [&[data-selected]_.text-muted-foreground]:text-(--koyori-color-text-secondary)"
    :class="selected ? 'bg-(--koyori-color-surface-hover)' : undefined"
    :aria-current="selected ? 'true' : undefined"
    :data-selected="selected ? '' : undefined"
    data-testid="finding-row"
  >
    <div class="flex min-h-11 items-center gap-2 px-3 py-1.5">
      <Badge
        :label="SEVERITY_LABELS[finding.severity]"
        :tone="blocksMerge ? 'danger' : 'neutral'"
        appearance="outline"
        size="sm"
      />
      <Badge :label="STATE_LABELS[finding.state]" tone="neutral" appearance="outline" size="sm" />
      <span class="text-muted-foreground shrink-0 text-xs">R{{ finding.round }}</span>
      <span v-if="finding.line" class="text-muted-foreground shrink-0 font-mono text-xs">
        :{{ finding.line }}
      </span>
      <a
        class="min-w-0 flex-1 truncate text-sm underline-offset-4 hover:underline"
        :class="unresolved ? 'font-semibold' : 'text-(--koyori-color-text-secondary)'"
        :href="href"
        data-testid="finding-title"
        @click="onTitleClick"
      >
        {{ finding.title }}
      </a>
      <span v-if="copied" role="status" class="text-muted-foreground shrink-0 text-xs">
        コピーしました
      </span>
      <ReviewDeferredTaskLink
        v-if="finding.state === 'deferred' && finding.deferred_task_id"
        :tenant-id="tenantId"
        :tenant-slug="tenantSlug"
        :project-id="projectId"
        :project-key="projectKey"
        :task-id="finding.deferred_task_id"
      />
      <Dropdown
        variant="ghost"
        :ariaLabel="`「${finding.title}」のその他の操作`"
        :items="menuItems"
        :on-select="onMenuSelect"
        top-layer
      >
        <template #icon><EllipsisIcon /></template>
      </Dropdown>
    </div>

    <div v-if="selected" class="flex flex-col gap-3 px-3 pb-3" data-testid="finding-detail">
      <ReviewFindingBody :body="finding.body" :finding-id="finding.id" :tenant="tenantSlug" />

      <div
        v-if="form"
        class="bg-card flex flex-col gap-3 rounded-md border p-4"
        data-finding-form
        :data-testid="`finding-form-${openForm}`"
      >
        <div class="flex flex-col gap-0.5">
          <h3 class="text-sm font-semibold">{{ form.title }}</h3>
          <p class="text-muted-foreground text-xs">{{ form.description }}</p>
        </div>
        <Field
          :id="`finding-note-${finding.id}`"
          label="理由"
          :required="form.required"
          required-mark="asterisk"
        >
          <Textarea
            :rows="3"
            :max-length="NOTE_MAX_LENGTH"
            :placeholder="form.placeholder"
            :value="note"
            :on-value-change="(v: string) => (note = v)"
          />
        </Field>
        <p
          v-if="openForm === 'defer'"
          class="text-muted-foreground flex items-center gap-1.5 text-xs"
        >
          <PhInfo class="size-3.5 shrink-0" aria-hidden="true" />
          繰り延べると、あとで扱うタスクを自動で作ります。
        </p>
        <div class="flex flex-wrap gap-2">
          <Button
            type="button"
            :variant="form.variant"
            :disabled="!canConfirm"
            :label="form.confirm"
            data-testid="finding-form-confirm"
            @click="confirmForm"
          >
            <template #icon><component :is="form.icon" class="size-4" /></template>
          </Button>
          <Button type="button" variant="ghost" label="キャンセル" @click="closeForm" />
        </div>
      </div>

      <div
        v-else-if="actions.left.length > 0 || actions.right.length > 0"
        class="flex flex-wrap items-center gap-2"
        data-testid="finding-actions"
      >
        <template v-for="action in actions.left" :key="`${action.to}-${action.label}`">
          <Button
            type="button"
            :variant="action.variant"
            :disabled="pending || action.disabledReason !== null"
            :label="action.label"
            @click="onAction(action)"
          >
            <template #icon><component :is="ICONS[action.icon]" class="size-4" /></template>
          </Button>
          <span v-if="action.disabledReason" class="text-muted-foreground text-xs">
            {{ action.disabledReason }}
          </span>
        </template>
        <div v-if="actions.right.length > 0" class="ml-auto flex flex-wrap items-center gap-2">
          <Button
            v-for="action in actions.right"
            :key="`${action.to}-${action.label}`"
            type="button"
            :variant="action.variant"
            :tone="action.danger ? 'danger' : 'default'"
            :disabled="pending"
            :label="action.label"
            @click="onAction(action)"
          >
            <template #icon><component :is="ICONS[action.icon]" class="size-4" /></template>
          </Button>
        </div>
      </div>

      <Alert v-if="error" :message="error" />

      <ul
        v-if="showHistory && finding.transitions.length > 0"
        class="flex flex-col gap-1"
        data-testid="finding-history"
      >
        <li
          v-for="history in finding.transitions"
          :key="history.id"
          class="text-muted-foreground text-xs"
        >
          {{ transitionText(history) }} · {{ relativeTime(history.created_at) }}
          <template v-if="history.note"> · {{ history.note }}</template>
        </li>
      </ul>
      <p
        v-else-if="lastTransition"
        class="text-muted-foreground flex items-center gap-1.5 text-xs"
        data-testid="finding-last-transition"
      >
        <PhClock class="size-3.5 shrink-0" aria-hidden="true" />
        <span>
          {{ transitionText(lastTransition) }} · {{ relativeTime(lastTransition.created_at) }}
          <template v-if="lastTransition.note"> · {{ lastTransition.note }}</template>
        </span>
      </p>
    </div>
  </li>
</template>

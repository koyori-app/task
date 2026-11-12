<script setup lang="ts">
import { ArrowRight, CalendarClock, CalendarDays, X } from '@lucide/vue';
import { computed, nextTick, ref } from 'vue';
import { Calendar } from '@koyori-app/ui-vue';

import AnchoredPanel from '@/components/reactions/AnchoredPanel.vue';
import { CHIP_CLASS, CHIP_EMPTY_CLASS, CHIP_OPEN_CLASS } from '@/lib/task-chip';
import { formatShortDate } from '@/lib/task-date';

/**
 * 開始予定日・期限・最終期限を 1 つにまとめたチップ。
 *
 * koyori の DatePicker は引き金の中身を替えられず 3 つを 1 つにまとめられないので、
 * チップの button + AnchoredPanel + koyori の Calendar で組む。パネルの上に 3 行（値と消す ×）、
 * 下に選んでいる行のカレンダーを置く。日を選んでもパネルは閉じない（続けて他の行を設定できる）。
 * 値は YYYY-MM-DD、未設定は空文字。
 */
type Key = 'plannedStartOn' | 'softDeadline' | 'hardDeadline';

const FIELDS = [
  { key: 'plannedStartOn', label: '開始予定日', short: '開始予定', icon: CalendarDays },
  { key: 'softDeadline', label: '期限', short: '期限', icon: CalendarDays },
  { key: 'hardDeadline', label: '最終期限', short: '最終期限', icon: CalendarClock },
] as const;

const plannedStartOn = defineModel<string>('plannedStartOn', { required: true });
const softDeadline = defineModel<string>('softDeadline', { required: true });
const hardDeadline = defineModel<string>('hardDeadline', { required: true });
defineProps<{
  /** 引き金の id（ラベルの `for` などが指す）。複数の根を持つので、属性としては落ちてこない */
  id?: string;
  /** 開始予定が期限より後 */
  invalid?: boolean;
}>();

const values = computed<Record<Key, string>>(() => ({
  plannedStartOn: plannedStartOn.value,
  softDeadline: softDeadline.value,
  hardDeadline: hardDeadline.value,
}));

function setValue(key: Key, value: string) {
  if (key === 'plannedStartOn') plannedStartOn.value = value;
  else if (key === 'softDeadline') softDeadline.value = value;
  else hardDeadline.value = value;
}

const open = ref(false);
const active = ref<Key>('softDeadline');
const anchor = ref<HTMLButtonElement | null>(null);
const panel = ref<HTMLElement | null>(null);

const activeField = computed(() => FIELDS.find((field) => field.key === active.value)!);
const activeValue = computed(() => values.value[active.value]);
const anySet = computed(() => FIELDS.some((field) => values.value[field.key]));

const triggerLabel = computed(() =>
  anySet.value
    ? `日付: ${FIELDS.map(
        (field) =>
          `${field.label} ${values.value[field.key] ? formatShortDate(values.value[field.key]) : '未設定'}`,
      ).join('、')}`
    : '日付',
);

function close() {
  open.value = false;
}

function toggle() {
  if (open.value) {
    close();
    return;
  }
  // 開くたびに期限の行へ戻す
  active.value = 'softDeadline';
  open.value = true;
  void nextTick(() => {
    panel.value?.querySelector<HTMLElement>(`[data-date-field="${active.value}"]`)?.focus();
  });
}

function closeAndRefocus() {
  close();
  anchor.value?.focus();
}

/** 押してフォーカスを受ける要素（日のセルの `td[tabindex]` を含む） */
const FOCUSABLE = 'button, a[href], input, select, textarea, [tabindex]';

/**
 * 値を消した行の × は、消えると同時に DOM から外れ、フォーカスが body へ落ちる。次の Esc がパネルを通らず
 * 作成ダイアログの cancel になるので、× が外れる前に、同じ行の日付ボタンへフォーカスを移す
 */
function clearValue(key: Key, event: MouseEvent) {
  const row = (event.currentTarget as Element).closest('[data-date-row]');
  row?.querySelector<HTMLElement>(`[data-date-field="${key}"]`)?.focus();
  setValue(key, '');
}

/**
 * shortcut: WebKit はボタンを押してもフォーカスせず dialog へ移すので、AnchoredPanel の lightDismiss が
 * 外へ出たと見てパネルを閉じ、月送りとクリアが効かない。ボタン以外（見出し・曜日・余白・警告文）を押しても
 * フォーカスが dialog へ移るので、フォーカスを受ける要素以外はフォーカスを動かさない。koyori 側で直ったら外す
 */
function keepFocusOnPress(event: MouseEvent) {
  const target = event.target as Element;
  const focusable = target.closest<HTMLElement>(FOCUSABLE);
  if (!focusable || !panel.value?.contains(focusable)) {
    event.preventDefault();
    return;
  }
  // ボタンは WebKit が自分ではフォーカスしないので、ここで移す。それ以外は既定の動作に任せる
  if (focusable instanceof HTMLButtonElement) {
    event.preventDefault();
    focusable.focus();
  }
}
</script>

<template>
  <button
    :id="id"
    ref="anchor"
    type="button"
    :class="[
      CHIP_CLASS,
      anySet ? '' : CHIP_EMPTY_CLASS,
      open ? CHIP_OPEN_CLASS : '',
      invalid ? 'border-destructive text-destructive' : '',
    ]"
    aria-haspopup="dialog"
    :aria-expanded="open"
    :aria-label="triggerLabel"
    :aria-invalid="invalid || undefined"
    @click="toggle"
  >
    <CalendarDays class="size-3.5 shrink-0" aria-hidden="true" />
    <template v-if="anySet">
      <span :class="plannedStartOn ? '' : 'text-(--koyori-color-text-muted)'">{{
        plannedStartOn ? formatShortDate(plannedStartOn) : '開始予定'
      }}</span>
      <ArrowRight class="size-3 shrink-0 opacity-60" aria-hidden="true" />
      <span :class="softDeadline ? '' : 'text-(--koyori-color-text-muted)'">{{
        softDeadline ? formatShortDate(softDeadline) : '期限'
      }}</span>
      <template v-if="hardDeadline">
        <ArrowRight class="size-3 shrink-0 opacity-60" aria-hidden="true" />
        <span>{{ formatShortDate(hardDeadline) }}</span>
      </template>
    </template>
    <span v-else>日付</span>
  </button>

  <AnchoredPanel v-if="open" :anchor="anchor" light-dismiss @dismiss="close">
    <div
      ref="panel"
      role="dialog"
      aria-label="日付"
      class="w-[17rem] rounded-lg border bg-(--koyori-color-surface-raised) p-1.5 text-[0.8125rem] shadow-lg"
      @keydown.esc.prevent.stop="closeAndRefocus"
      @mousedown="keepFocusOnPress"
    >
      <div
        v-for="field in FIELDS"
        :key="field.key"
        data-date-row
        class="flex items-center rounded-md"
        :class="active === field.key ? 'bg-(--koyori-color-surface-hover)' : ''"
      >
        <button
          type="button"
          :data-date-field="field.key"
          :aria-pressed="active === field.key"
          class="flex h-[30px] flex-1 items-center gap-2 rounded-md px-2 text-left focus-visible:outline-2 focus-visible:outline-(--koyori-focus-color)"
          @click="active = field.key"
        >
          <component :is="field.icon" class="size-3.5 shrink-0" aria-hidden="true" />
          <span>{{ field.short }}</span>
          <span
            class="ml-auto"
            :class="values[field.key] ? '' : 'text-(--koyori-color-text-muted)'"
            >{{ values[field.key] ? formatShortDate(values[field.key]) : '未設定' }}</span
          >
        </button>
        <button
          v-if="values[field.key]"
          type="button"
          :aria-label="`${field.label}を消す`"
          class="grid size-[30px] shrink-0 place-items-center opacity-60 hover:opacity-100 focus-visible:outline-2 focus-visible:outline-(--koyori-focus-color)"
          @click="clearValue(field.key, $event)"
        >
          <X class="size-3.5" aria-hidden="true" />
        </button>
      </div>

      <div class="-mx-1.5 my-1.5 border-t" />
      <p v-if="invalid" class="px-2 pb-1 text-xs text-destructive">
        開始予定日が期限より後になっています
      </p>
      <Calendar
        :key="active"
        :label="activeField.label"
        :value="activeValue"
        :on-value-change="(v: string) => setValue(active, v)"
      />
    </div>
  </AnchoredPanel>
</template>

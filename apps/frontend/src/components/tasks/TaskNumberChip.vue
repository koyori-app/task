<script setup lang="ts">
import { computed, ref, type Component } from 'vue';

import { CHIP_CLASS, CHIP_EMPTY_CLASS } from '@/lib/task-chip';

/**
 * 見積・進捗のように数を直接入力するチップ。
 *
 * チップそのものを label にし、中に枠なしの number 入力を常に置く（幅 0 でも Tab で届く）。
 * 送信時の `validity.badInput` の検証が、入力欄が DOM にあることを前提にしているので、
 * ポップオーバーには入れない（閉じると消えてしまう）。
 */
const model = defineModel<string>({ required: true });
defineProps<{
  /** チップに出す名（「見積」「進捗」） */
  label: string;
  /** 入力欄の読み上げ名（「見積もり」「進捗率」） */
  ariaLabel: string;
  name: string;
  /** 数の後ろに出す単位（「分」「%」） */
  unit: string;
  icon: Component;
  min: number;
  max: number;
}>();

const focused = ref(false);
/** 「1e」のように数値にならない入力。value は空で届くので、入力があることを別に覚える */
const badInput = ref(false);
const active = computed(() => focused.value || model.value !== '' || badInput.value);

function onInput(event: Event) {
  const input = event.target as HTMLInputElement;
  badInput.value = input.validity.badInput;
  model.value = input.value;
}

function onBlur(event: FocusEvent) {
  focused.value = false;
  badInput.value = (event.target as HTMLInputElement).validity.badInput;
}
</script>

<template>
  <label
    :class="[
      CHIP_CLASS,
      'cursor-text focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-(--koyori-focus-color)',
      active ? '' : CHIP_EMPTY_CLASS,
    ]"
  >
    <component :is="icon" class="size-3.5 shrink-0" aria-hidden="true" />
    <span>{{ label }}</span>
    <input
      type="number"
      :name="name"
      :min="min"
      :max="max"
      step="1"
      inputmode="numeric"
      :aria-label="ariaLabel"
      :value="model"
      :class="[
        'bg-transparent tabular-nums outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none',
        active ? '' : 'w-0 p-0 opacity-0',
      ]"
      :style="active ? { width: `${Math.max(String(model).length, 2)}ch` } : undefined"
      @focus="focused = true"
      @blur="onBlur"
      @input="onInput"
    />
    <span v-if="active">{{ unit }}</span>
  </label>
</template>

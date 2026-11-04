<script setup lang="ts">
import { SmilePlus } from '@lucide/vue';
import { ref } from 'vue';

import AnchoredPanel from './AnchoredPanel.vue';
import EmojiPicker from './EmojiPicker.vue';

/** リアクションの追加ボタンとピッカー。札の行の末尾と、コメントの ⋯ の隣で使う */
withDefaults(
  defineProps<{
    /** 自分が押している絵文字 */
    selected?: string[];
    /** 送信中など。`disabled` 属性にするとピッカーから戻したフォーカスを失う */
    disabled?: boolean;
  }>(),
  { selected: () => [], disabled: false },
);

const emit = defineEmits<{ select: [emoji: string] }>();

const buttonRef = ref<HTMLButtonElement | null>(null);
const open = ref(false);

function close(restoreFocus: boolean) {
  open.value = false;
  if (restoreFocus) buttonRef.value?.focus();
}

function onSelect(emoji: string) {
  close(true);
  emit('select', emoji);
}
</script>

<template>
  <button
    ref="buttonRef"
    type="button"
    class="reaction-add"
    aria-label="リアクションを追加"
    aria-haspopup="dialog"
    :aria-expanded="open"
    :aria-disabled="disabled || undefined"
    @click="disabled || (open = !open)"
  >
    <SmilePlus :size="16" aria-hidden="true" />
  </button>
  <AnchoredPanel v-if="open" :anchor="buttonRef" dismiss-on-focus-out @dismiss="close(false)">
    <EmojiPicker :selected="selected" @select="onSelect" @close="close(true)" />
  </AnchoredPanel>
</template>

<style scoped>
.reaction-add {
  display: inline-flex;
  width: 28px;
  height: 24px;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  color: var(--koyori-color-text-secondary);
  touch-action: manipulation;
}

.reaction-add[aria-disabled='true'] {
  color: var(--koyori-color-text-disabled);
  cursor: default;
}

@media (hover: hover) {
  .reaction-add:not([aria-disabled='true']):hover {
    background: var(--koyori-color-surface-hover);
    color: var(--koyori-color-text);
  }
}

.reaction-add:not([aria-disabled='true']):active {
  background: var(--koyori-color-surface-active);
}

.reaction-add:focus-visible {
  outline: var(--koyori-focus-width) solid var(--koyori-focus-color);
  outline-offset: var(--koyori-focus-offset);
}

@media (hover: none) {
  .reaction-add {
    width: 36px;
    height: 32px;
  }
}
</style>

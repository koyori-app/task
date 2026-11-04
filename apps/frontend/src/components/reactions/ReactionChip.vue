<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue';

import { emojiName, preloadEmojiData } from '@/lib/emoji-data';
import { formatReactionCount, reactionChipLabel } from '@/lib/reactions';

const LONG_PRESS_MS = 500;
/** これ以上指が動いたらスクロールと見なして長押しをやめる */
const LONG_PRESS_MOVE_TOLERANCE_PX = 10;

const props = withDefaults(
  defineProps<{
    emoji: string;
    count: number;
    reacted: boolean;
    /**
     * 押せない時（権限なし・送信中）。`disabled` 属性だとフォーカスとカードを失い、
     * 送信の直後にフォーカスが飛ぶので `aria-disabled` にする。
     */
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{
  toggle: [];
  /** タッチ端末の長押し（全員の一覧を開く） */
  longPress: [chip: HTMLElement];
}>();

const label = computed(() =>
  reactionChipLabel(emojiName(props.emoji) ?? props.emoji, props.count, props.reacted),
);

let pressTimer: ReturnType<typeof setTimeout> | undefined;
let pressStart: { x: number; y: number } | null = null;
const longPressed = ref(false);

function cancelPress() {
  clearTimeout(pressTimer);
  pressStart = null;
}

function onPointerDown(event: PointerEvent) {
  longPressed.value = false;
  if (event.pointerType !== 'touch') return;
  const chip = event.currentTarget as HTMLElement;
  pressStart = { x: event.clientX, y: event.clientY };
  clearTimeout(pressTimer);
  pressTimer = setTimeout(() => {
    pressStart = null;
    longPressed.value = true;
    emit('longPress', chip);
  }, LONG_PRESS_MS);
}

function onPointerMove(event: PointerEvent) {
  if (!pressStart) return;
  const moved = Math.hypot(event.clientX - pressStart.x, event.clientY - pressStart.y);
  if (moved > LONG_PRESS_MOVE_TOLERANCE_PX) cancelPress();
}

function onClick() {
  // 長押しの指を離した時の click で切り替えない
  if (longPressed.value) {
    longPressed.value = false;
    return;
  }
  if (!props.disabled) emit('toggle');
}

function onContextMenu(event: Event) {
  // Android は長押しでメニューを出すので、全員の一覧と重ならないよう止める
  if (pressStart || longPressed.value) event.preventDefault();
}

onBeforeUnmount(cancelPress);
</script>

<template>
  <button
    type="button"
    class="reaction-chip"
    data-reaction-chip
    :data-emoji="emoji"
    :data-reacted="reacted || undefined"
    :aria-pressed="reacted"
    :aria-disabled="disabled || undefined"
    :aria-label="label"
    @click="onClick"
    @focus="preloadEmojiData"
    @pointerenter="preloadEmojiData"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="cancelPress"
    @pointercancel="cancelPress"
    @contextmenu="onContextMenu"
  >
    <span class="reaction-chip__surface">
      <span aria-hidden="true">{{ emoji }}</span>
      <span class="reaction-chip__count" aria-hidden="true">{{ formatReactionCount(count) }}</span>
    </span>
  </button>
</template>

<style scoped>
.reaction-chip {
  --chip-bg: var(--koyori-color-surface);
  --chip-border: var(--koyori-color-border);
  --chip-count: var(--koyori-color-text-secondary);
  display: inline-flex;
  height: 24px;
  border-radius: 8px;
  touch-action: manipulation;
  -webkit-touch-callout: none;
  user-select: none;
}

.reaction-chip[aria-disabled='true'] {
  cursor: default;
}

.reaction-chip[data-reacted] {
  --chip-bg: var(--koyori-color-selection);
  --chip-border: var(--koyori-color-selection-indicator);
  --chip-count: var(--koyori-color-text);
}

@media (hover: hover) {
  .reaction-chip:not([aria-disabled='true']):hover {
    --chip-bg: var(--koyori-color-surface-active);
    --chip-border: var(--koyori-color-border-strong);
  }

  .reaction-chip[data-reacted]:not([aria-disabled='true']):hover {
    --chip-bg: var(--koyori-color-accent-hover);
    --chip-border: var(--koyori-color-selection-indicator);
  }
}

.reaction-chip:focus-visible {
  outline: var(--koyori-focus-width) solid var(--koyori-focus-color);
  outline-offset: var(--koyori-focus-offset);
}

/* Koyori の Button と同じく、当たり判定は残して見た目だけ縮める */
.reaction-chip__surface {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: 1px solid var(--chip-border);
  border-radius: inherit;
  padding: 0 8px 0 6px;
  background: var(--chip-bg);
  font-size: 14px;
  line-height: 1;
  transition:
    background-color var(--koyori-motion-fast) var(--koyori-ease-out),
    border-color var(--koyori-motion-fast) var(--koyori-ease-out);
}

@media (prefers-reduced-motion: no-preference) {
  .reaction-chip:active:not([aria-disabled='true']) .reaction-chip__surface {
    transform: scale(var(--button-press-scale, 0.96));
  }
}

.reaction-chip__count {
  color: var(--chip-count);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

@media (hover: none) {
  .reaction-chip {
    height: 32px;
  }
}
</style>

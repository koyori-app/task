<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';

import { placeAnchored } from '@/lib/anchored-position';

/**
 * 開いたボタンの近くに出す、top layer の入れ物。Koyori の Dropdown の `topLayer` と同じく
 * Popover API（manual）で出すので、overflow の中やタスク詳細の dialog の中でも切れない。
 */
const props = withDefaults(
  defineProps<{
    anchor: HTMLElement | null;
    /** フォーカスが外へ移ったら閉じる（ピッカーと全員の一覧）。カードは Bar が自分で閉じる */
    dismissOnFocusOut?: boolean;
  }>(),
  { dismissOnFocusOut: false },
);

const emit = defineEmits<{ dismiss: [] }>();

const rootRef = ref<HTMLElement | null>(null);

function reposition() {
  const root = rootRef.value;
  if (!root || !props.anchor) return;
  const rect = root.getBoundingClientRect();
  const { left, top } = placeAnchored(
    props.anchor.getBoundingClientRect(),
    { width: rect.width, height: rect.height },
    { width: window.innerWidth, height: window.innerHeight },
  );
  root.style.left = `${left}px`;
  root.style.top = `${top}px`;
}

function isInside(target: EventTarget | null) {
  return (
    target instanceof Node && (rootRef.value?.contains(target) || props.anchor?.contains(target))
  );
}

function onPointerDownOutside(event: PointerEvent) {
  if (!isInside(event.target)) emit('dismiss');
}

function onScroll(event: Event) {
  // パネルの中のスクロールでは動かさない
  if (!(event.target instanceof Node && rootRef.value?.contains(event.target))) reposition();
}

function onFocusOut(event: FocusEvent) {
  // relatedTarget が無いのは窓自体からフォーカスが外れた時。戻れば続きができるので閉じない
  if (props.dismissOnFocusOut && event.relatedTarget && !isInside(event.relatedTarget)) {
    emit('dismiss');
  }
}

onMounted(() => {
  const root = rootRef.value;
  if (root && typeof root.showPopover === 'function') {
    root.popover = 'manual';
    root.showPopover();
  }
  reposition();
  document.addEventListener('pointerdown', onPointerDownOutside, true);
  window.addEventListener('scroll', onScroll, true);
  window.addEventListener('resize', reposition);
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onPointerDownOutside, true);
  window.removeEventListener('scroll', onScroll, true);
  window.removeEventListener('resize', reposition);
});
</script>

<template>
  <div ref="rootRef" class="anchored-panel" @focusout="onFocusOut">
    <slot />
  </div>
</template>

<style scoped>
/* popover の UA 既定（中央寄せ・枠・Canvas 地）を消す。影を切らないよう overflow も戻す */
.anchored-panel {
  position: fixed;
  inset: auto;
  z-index: var(--koyori-z-menu);
  margin: 0;
  border: 0;
  padding: 0;
  overflow: visible;
  background: transparent;
  color: var(--koyori-color-text);
}
</style>

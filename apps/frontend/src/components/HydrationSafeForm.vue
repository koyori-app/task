<script setup lang="ts">
import { useHydrated } from '@/composables/useHydrated';

const emit = defineEmits<{
  submit: [event: SubmitEvent];
}>();

const isHydrated = useHydrated();

function handleSubmit(event: SubmitEvent) {
  if (!isHydrated.value) {
    event.preventDefault();
    return;
  }

  emit('submit', event);
}

function preventPrehydrationEnter(event: KeyboardEvent) {
  if (!isHydrated.value) {
    event.preventDefault();
  }
}
</script>

<template>
  <form
    novalidate
    :data-hydrated="isHydrated ? 'true' : 'false'"
    :onsubmit.attr="isHydrated ? null : 'return false;'"
    @submit.prevent="handleSubmit"
    @keydown.enter="preventPrehydrationEnter"
  >
    <!--
      コメントは根の外（`<template>` 直下の先頭）に置かないこと。本番ビルドでは
      コメントが残って根がフラグメントになり、呼び出し側の class などの fallthrough
      属性が黙って落ちる（dev は filterSingleRoot が補正するので再現しない）。

      novalidate: ネイティブの制約検証（type="email" など）が submit を先に止めると
      @submit ハンドラーが呼ばれず、フォーム側が出したいエラー表示に到達できない。
      検証はどのフォームも TanStack Form 側に一本化している
    -->
    <slot :is-hydrated="isHydrated" />
  </form>
</template>

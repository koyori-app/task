<script setup lang="ts">
import type { ButtonProps } from '@koyori-app/ui-vue';
import { Button } from '@koyori-app/ui-vue';

// template の class: 入力欄の中に収まるよう、koyori の高さのトークンをこのボタンだけ縮める
const props = withDefaults(
  defineProps<{
    variant?: ButtonProps['variant'];
    /** アイコンだけのボタンなので、読み上げる名前を必ず受ける */
    ariaLabel: string;
  }>(),
  { variant: 'ghost' },
);
</script>

<template>
  <Button
    :variant="props.variant"
    :ariaLabel="props.ariaLabel"
    class="[--koyori-control-height-sm:24px]"
  >
    <template #icon><slot /></template>
  </Button>
</template>

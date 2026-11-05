<script setup lang="ts">
import { Button } from '@koyori-app/ui-vue';

/**
 * 読み込みの失敗（5xx・通信断）の表示と再試行。「なし」「見つかりません」とは分けて出す。
 * 再試行の中身（query の refetch）は呼び出し側が `retry` で受ける。
 */
withDefaults(defineProps<{ message?: string }>(), { message: '読み込みに失敗しました' });
defineEmits<{ retry: [] }>();
</script>

<template>
  <div
    role="alert"
    class="flex flex-col items-center gap-2 py-8 text-center text-sm text-destructive"
    data-testid="load-error"
  >
    <p>{{ message }}</p>
    <Button variant="tertiary" label="再試行" @click="$emit('retry')" />
  </div>
</template>

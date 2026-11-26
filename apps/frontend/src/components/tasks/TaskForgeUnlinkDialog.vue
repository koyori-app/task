<script setup lang="ts">
import { Button, Dialog } from '@koyori-app/ui-vue';

import type { TaskForgePullRequest } from '@/composables/useTaskForge';

/** PR のリンクを外す確認。題の上のピルとリポジトリ欄が同じものを出す */
defineProps<{
  target: TaskForgePullRequest | null;
  pending: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  cancel: [];
  confirm: [];
}>();
</script>

<template>
  <Dialog
    :open="target !== null"
    title="PR のリンクを外しますか？"
    :description="
      target
        ? `#${target.number} とこのタスクのリンクをすべて外します。PR のタイトル・本文にタスクの ID が残っていると、次に PR が更新されたときに再びリンクされます。`
        : ''
    "
    :on-close="() => emit('cancel')"
  >
    <p v-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
    <template #actions>
      <Button
        type="button"
        variant="tertiary"
        :disabled="pending"
        label="キャンセル"
        @click="emit('cancel')"
      />
      <Button
        type="button"
        variant="danger"
        :disabled="pending"
        :label="pending ? '外しています…' : 'リンクを外す'"
        @click="emit('confirm')"
      />
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { navigate } from 'vike/client/router';

import { taskDetailHref } from '@/lib/task-display';

const props = defineProps<{
  tenantDisplayId: string;
  projectKey: string;
  seqId: number;
  title: string;
  /** true のとき、素の左クリックはフルページ遷移でなく select emit（分割ビューでの inline 選択）にする */
  inlineSelect?: boolean;
}>();

const emit = defineEmits<{
  select: [seqId: number];
}>();

function navigateToTask(event: MouseEvent) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  event.preventDefault();
  if (props.inlineSelect) {
    emit('select', props.seqId);
    return;
  }
  void navigate(taskDetailHref(props.tenantDisplayId, props.projectKey, props.seqId));
}
</script>

<template>
  <a
    :href="taskDetailHref(tenantDisplayId, projectKey, seqId)"
    class="truncate text-sm text-primary hover:underline after:absolute after:inset-0 after:content-['']"
    @click="navigateToTask"
  >
    {{ title }}
  </a>
</template>

<script setup lang="ts">
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import TaskDetailPane from '@/components/tasks/TaskDetailPane.vue';
import {
  shouldKeepOverlayOnEscape,
  shouldKeepOverlayOnPointerDownOutside,
} from '@/components/tasks/overlay-dismiss-guard';

/**
 * List 表示のタスク詳細。分割ビューではなくページに重ねて出す。
 *
 * 中身は分割ビューと同じ {@link TaskDetailPane}（＝ useTaskDetail + TaskDetailHub）で、
 * 器だけを差し替える。タイトルの変更はここでしかできない導線なので、詳細の機能は
 * 削らずそのまま載せる。
 */
defineProps<{
  open: boolean;
  tenantDisplayId: string;
  projectKey: string;
  /** URL と同じ seq key 形式（例: "ENG-42"） */
  taskId: string;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  'open-task': [task: import('@/generated/api').components['schemas']['TaskResponse']];
}>();

// 入力途中・IME 変換中・入れ子の <dialog> の間は閉じない（下書きを器ごと消さない）
function onEscapeKeyDown(event: KeyboardEvent) {
  if (shouldKeepOverlayOnEscape(event)) event.preventDefault();
}

// 1 回目は入力欄の blur だけ起こし、2 回目で閉じる
function onPointerDownOutside(event: Event) {
  if (shouldKeepOverlayOnPointerDownOutside(document.activeElement)) event.preventDefault();
}
</script>

<template>
  <Dialog :open="open" @update:open="(value) => emit('update:open', value)">
    <DialogContent
      class="flex h-[calc(100vh-3rem)] max-w-[70rem] flex-col gap-0 overflow-hidden p-0 sm:max-w-[70rem]"
      @escape-key-down="onEscapeKeyDown"
      @pointer-down-outside="onPointerDownOutside"
    >
      <!-- Dialog はアクセシブルな名前を要求する。見出しは詳細側が出すので視覚的には隠す -->
      <DialogTitle class="sr-only">タスクの詳細</DialogTitle>
      <!--
        v-if を付けない。閉じるとき即座に中身を消すと、Dialog の退場アニメーションが
        空の箱で再生される。ダイアログが閉じ切れば reka-ui が DOM ごと外すので、
        閉じている間にペインが動き続けることはない。
      -->
      <TaskDetailPane
        layout="page"
        :show-close-button="false"
        :tenant-display-id="tenantDisplayId"
        :project-key="projectKey"
        :task-id="taskId"
        @close="emit('update:open', false)"
        @open-task="emit('open-task', $event)"
      />
    </DialogContent>
  </Dialog>
</template>

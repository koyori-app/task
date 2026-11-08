<script setup lang="ts">
import { Dialog } from '@koyori-app/ui-vue';
import TaskDetailPane, { type TaskRefLink } from '@/components/tasks/TaskDetailPane.vue';
import {
  shouldKeepOverlayOnEscape,
  shouldKeepOverlayOnPointerDownOutside,
} from '@/components/tasks/overlay-dismiss-guard';

/**
 * タスク一覧の List 表示とカレンダーのタスク詳細。分割ビューではなくページに重ねて出す。
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
  'open-task-ref': [ref: TaskRefLink];
}>();

// 入力途中・IME 変換中は Esc で閉じない（下書きを器ごと消さない）。
// koyori の Dialog は Esc を dialog の cancel で受けて onClose を呼ぶが、onClose に event は来ない。
// そこで keydown の既定動作を止めて cancel そのものを起こさせない（cancel を止めるだけだと、
// Chromium は操作を挟まない 2 回目の Esc の cancel を止めさせずに閉じてしまう）
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && shouldKeepOverlayOnEscape(event)) event.preventDefault();
}

// 背景の pointerdown（と止めなかった Esc）で呼ばれる。pointerdown の時点ではまだ入力欄に
// フォーカスがあるので、1 回目は入力欄の blur だけ起こし、2 回目で閉じる
function onClose() {
  if (shouldKeepOverlayOnPointerDownOutside(document.activeElement)) return;
  emit('update:open', false);
}
</script>

<template>
  <div
    class="contents [--koyori-dialog-width:70rem] [--koyori-dialog-height:calc(100vh-3rem)]"
    @keydown="onKeydown"
  >
    <!-- 見出しは詳細側が出すので、title は plain で視覚的に隠れてダイアログの名前にだけ残る -->
    <Dialog :open="open" title="タスクの詳細" plain :on-close="onClose">
      <!--
        koyori の Dialog は閉じても DOM に残る（退場アニメーションも無い）。閉じている間に
        ペインが動き続けないよう、v-if で中身を外す
      -->
      <TaskDetailPane
        v-if="open"
        layout="page"
        :show-close-button="false"
        :tenant-display-id="tenantDisplayId"
        :project-key="projectKey"
        :task-id="taskId"
        @close="emit('update:open', false)"
        @open-task="emit('open-task', $event)"
        @open-task-ref="emit('open-task-ref', $event)"
      />
    </Dialog>
  </div>
</template>

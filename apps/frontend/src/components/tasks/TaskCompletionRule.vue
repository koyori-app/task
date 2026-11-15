<script setup lang="ts">
import { computed } from 'vue';
import { Button, Checkbox } from '@koyori-app/ui-vue';
import type { components } from '@/generated/api';

/**
 * タスクの「完了条件」（`require_all_assignees_done`）。
 *
 * on のタスクは、担当者全員が自分の分を完了にして初めて完了になる。
 * 切り替えと、ログイン中の利用者自身の分の完了・取り消しをここで扱う。
 * 他の担当者の分は本人にしか操作できないので、表示だけにする。
 */
type TaskDetail = components['schemas']['TaskDetailResponse'];

const props = defineProps<{
  task: Pick<TaskDetail, 'require_all_assignees_done' | 'assignees'>;
  /** ログイン中の利用者。担当者のときだけ自分の分を操作できる */
  currentUserId?: string | null;
  /** タスクが完了ステータスにいるか。完了済みの間は取り消せない */
  taskDone: boolean;
  updating?: boolean;
  error?: string | null;
}>();

const emit = defineEmits<{
  'update:requireAll': [value: boolean];
  complete: [];
  reopen: [];
}>();

const requireAll = computed(() => !!props.task.require_all_assignees_done);
const completedCount = computed(
  () => props.task.assignees.filter((assignee) => !!assignee.completed_at).length,
);
const mine = computed(() =>
  props.currentUserId
    ? props.task.assignees.find((assignee) => assignee.user.id === props.currentUserId)
    : undefined,
);
</script>

<template>
  <div class="flex flex-col gap-2" data-completion-rule>
    <Checkbox
      label="担当者全員の完了が必要"
      name="require-all-assignees-done"
      :checked="requireAll"
      :disabled="updating"
      :on-checked-change="(checked: boolean) => emit('update:requireAll', checked)"
    />

    <template v-if="requireAll">
      <p v-if="!task.assignees.length" class="text-xs text-muted-foreground">
        担当者を設定すると、全員の完了が必要になります
      </p>
      <template v-else>
        <p class="text-xs text-muted-foreground" data-completion-progress>
          {{ completedCount }} / {{ task.assignees.length }} 人が完了
        </p>
        <ul class="flex flex-col gap-0.5 text-xs" data-completion-assignees>
          <li
            v-for="assignee in task.assignees"
            :key="assignee.user.id"
            class="flex items-center gap-2"
          >
            <span class="truncate">{{ assignee.user.username }}</span>
            <span
              :class="assignee.completed_at ? 'text-foreground' : 'text-muted-foreground'"
              :data-completed="assignee.completed_at ? 'true' : 'false'"
            >
              {{ assignee.completed_at ? '完了' : '未完了' }}
            </span>
          </li>
        </ul>
        <div v-if="mine" class="flex">
          <Button
            v-if="!mine.completed_at"
            type="button"
            variant="secondary"
            label="自分の分を完了にする"
            :disabled="updating"
            @click="emit('complete')"
          />
          <Button
            v-else-if="!taskDone"
            type="button"
            variant="ghost"
            label="自分の完了を取り消す"
            :disabled="updating"
            @click="emit('reopen')"
          />
        </div>
      </template>
    </template>

    <p v-if="error" role="alert" class="text-xs text-destructive">{{ error }}</p>
  </div>
</template>

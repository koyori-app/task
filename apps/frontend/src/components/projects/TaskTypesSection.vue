<script setup lang="ts">
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { PhArrowDown, PhArrowUp, PhPencilSimple, PhPlus, PhTrash } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import { Button, Dialog } from '@koyori-app/ui-vue';
import TaskTypeFormDialog from '@/components/projects/TaskTypeFormDialog.vue';
import { apiClient, projectTaskTypesQueryOptions } from '@/lib/api-vue-query';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import type { components } from '@/generated/api';

type TaskTypeResponse = components['schemas']['TaskTypeResponse'];

const TASK_TYPES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/task-types' as const;
const TASK_TYPE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/task-types/{id}' as const;
const REORDER_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/task-types/reorder' as const;
const TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

const props = defineProps<{
  tenantId: string;
  projectId: string;
}>();

const queryClient = useQueryClient();
const isFormOpen = ref(false);
const editingType = ref<TaskTypeResponse | null>(null);
const deleteTarget = ref<TaskTypeResponse | null>(null);
const deleteError = ref<string | null>(null);

// 共有ヘルパーに寄せてキー構築・staleTime を一元管理する（#362 の branded 型を渡す。
// props は親で解決済みの UUID）。
// options は computed にして props に追従させる。素の呼び出しだと setup 時の値で
// 固定され、プロジェクトを切り替えたときに一覧だけが前のプロジェクトのまま残る
const taskTypesQuery = useQuery(
  computed(() =>
    projectTaskTypesQueryOptions(props.tenantId as TenantUuid, props.projectId as ProjectUuid),
  ),
);
const taskTypes = computed(() => taskTypesQuery.data.value ?? []);

const deleteMutation = apiClient.useMutation('delete', TASK_TYPE_PATH);
const reorderMutation = apiClient.useMutation('put', REORDER_PATH);
const reorderError = ref<string | null>(null);

function openCreate() {
  editingType.value = null;
  isFormOpen.value = true;
}

function openEdit(taskType: TaskTypeResponse) {
  editingType.value = taskType;
  isFormOpen.value = true;
}

function openDelete(taskType: TaskTypeResponse) {
  deleteError.value = null;
  deleteTarget.value = taskType;
}

function onDeleteClose() {
  // 削除リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (deleteMutation.isPending.value) return;
  deleteTarget.value = null;
}

async function confirmDelete() {
  const target = deleteTarget.value;
  if (!target) return;
  deleteError.value = null;
  try {
    await deleteMutation.mutateAsync({
      params: {
        path: { tenant_id: props.tenantId, project_id: props.projectId, id: target.id },
      },
    });
    // 付いていたタスクは種別が外れるので、タスクの表示も取り直す
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', TASK_TYPES_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', TASKS_PATH] }),
    ]);
    deleteTarget.value = null;
  } catch {
    deleteError.value = '種別を削除できませんでした';
  }
}

async function moveType(index: number, offset: -1 | 1) {
  const reordered = [...taskTypes.value];
  const target = index + offset;
  if (target < 0 || target >= reordered.length) return;
  [reordered[index], reordered[target]] = [reordered[target]!, reordered[index]!];
  reorderError.value = null;
  try {
    await reorderMutation.mutateAsync({
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
      body: { ids: reordered.map((taskType) => taskType.id) },
    });
    await queryClient.invalidateQueries({ queryKey: ['get', TASK_TYPES_PATH] });
  } catch {
    reorderError.value = '並び順を変更できませんでした';
  }
}
</script>

<template>
  <div>
    <div class="mb-5 flex items-center justify-between border-b pb-4">
      <div>
        <h2 class="text-xl font-semibold">種別</h2>
        <p class="mt-1 text-sm text-muted-foreground">
          タスクの種類（バグ・機能など）を管理します。1 つのタスクに種別は 1 つだけ付けられます。
        </p>
      </div>
      <Button type="button" variant="tertiary" label="新しい種別" @click="openCreate">
        <template #icon><PhPlus class="size-4" /></template>
      </Button>
    </div>

    <p v-if="taskTypesQuery.isPending.value" class="text-sm text-muted-foreground">読み込み中…</p>

    <p v-else-if="taskTypesQuery.isError.value" role="alert" class="text-sm text-destructive">
      種別を読み込めませんでした
    </p>

    <p v-else-if="taskTypes.length === 0" class="text-sm text-muted-foreground">
      種別はまだありません。「新しい種別」から作成できます。
    </p>

    <template v-else>
      <p v-if="reorderError" role="alert" class="mb-2 text-sm text-destructive">
        {{ reorderError }}
      </p>
      <ul class="overflow-hidden rounded-lg border">
        <li
          v-for="(taskType, index) in taskTypes"
          :key="taskType.id"
          class="flex items-center gap-3 border-b px-3.5 py-2.5 last:border-b-0"
        >
          <span
            class="inline-flex min-w-0 shrink-0 items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium"
          >
            <span
              class="size-2 shrink-0 rounded-full"
              :style="{ backgroundColor: taskType.color }"
              aria-hidden="true"
            />
            <span class="truncate">{{ taskType.name }}</span>
          </span>
          <span class="flex-1" />
          <Button
            type="button"
            variant="ghost"
            :disabled="index === 0 || reorderMutation.isPending.value"
            :ariaLabel="`種別「${taskType.name}」を上へ`"
            @click="moveType(Number(index), -1)"
          >
            <template #icon><PhArrowUp class="size-4" /></template>
          </Button>
          <Button
            type="button"
            variant="ghost"
            :disabled="index === taskTypes.length - 1 || reorderMutation.isPending.value"
            :ariaLabel="`種別「${taskType.name}」を下へ`"
            @click="moveType(Number(index), 1)"
          >
            <template #icon><PhArrowDown class="size-4" /></template>
          </Button>
          <Button
            type="button"
            variant="ghost"
            :ariaLabel="`種別「${taskType.name}」を編集`"
            @click="openEdit(taskType)"
          >
            <template #icon><PhPencilSimple class="size-4" /></template>
          </Button>
          <Button
            type="button"
            variant="ghost"
            :ariaLabel="`種別「${taskType.name}」を削除`"
            @click="openDelete(taskType)"
          >
            <template #icon><PhTrash class="size-4" /></template>
          </Button>
        </li>
      </ul>
    </template>

    <TaskTypeFormDialog
      v-if="isFormOpen"
      :tenant-id="tenantId"
      :project-id="projectId"
      :task-type="editingType"
      @close="isFormOpen = false"
    />

    <Dialog
      :open="deleteTarget !== null"
      title="種別を削除しますか？"
      :description="
        deleteTarget
          ? `「${deleteTarget.name}」を削除します。付いていたタスクは残り、種別だけが外れます。この操作は取り消せません。`
          : ''
      "
      :on-close="onDeleteClose"
    >
      <p v-if="deleteError" role="alert" class="text-sm text-destructive">{{ deleteError }}</p>
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="deleteMutation.isPending.value"
          label="キャンセル"
          @click="deleteTarget = null"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="deleteMutation.isPending.value"
          :label="deleteMutation.isPending.value ? '削除中…' : '削除する'"
          @click="confirmDelete"
        />
      </template>
    </Dialog>
  </div>
</template>

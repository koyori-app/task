<script setup lang="ts">
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { PhArrowDown, PhArrowUp, PhPlus, PhTrash } from '@phosphor-icons/vue';
import { computed, ref, watch } from 'vue';

import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import type { components } from '@/generated/api';
import { apiClient } from '@/lib/api-vue-query';

type ProjectStatus = components['schemas']['ProjectStatusResponse'];

const STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
const STATUS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses/{id}' as const;
const REORDER_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses/reorder' as const;

const props = defineProps<{
  tenantId: string;
  projectId: string;
}>();

const queryClient = useQueryClient();
const operationError = ref<string | null>(null);
const operationInFlight = ref(false);
const newName = ref('');
const newColor = ref('#64748b');
const deleteTarget = ref<ProjectStatus | null>(null);
const drafts = ref<Record<string, { name: string; color: string }>>({});

const pathParams = computed(() => ({ tenant_id: props.tenantId, project_id: props.projectId }));
const statusesQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', STATUSES_PATH, { params: { path: pathParams.value } }),
    retry: false,
  })),
);

const createMutation = apiClient.useMutation('post', STATUSES_PATH);
const updateMutation = apiClient.useMutation('put', STATUS_PATH);
const deleteMutation = apiClient.useMutation('delete', STATUS_PATH);
const reorderMutation = apiClient.useMutation('put', REORDER_PATH);

const statuses = computed<ProjectStatus[]>(() => statusesQuery.data.value ?? []);
const isMutating = computed(
  () =>
    operationInFlight.value ||
    createMutation.isPending.value ||
    updateMutation.isPending.value ||
    deleteMutation.isPending.value ||
    reorderMutation.isPending.value,
);

watch(
  statuses,
  (items) => {
    drafts.value = Object.fromEntries(
      items.map((status) => [status.id, { name: status.name, color: status.color }]),
    );
  },
  { immediate: true },
);

async function refresh() {
  await queryClient.invalidateQueries({ queryKey: ['get', STATUSES_PATH] });
}

async function run(message: string, action: () => Promise<unknown>) {
  operationError.value = null;
  operationInFlight.value = true;
  try {
    await action();
  } catch {
    operationError.value = message;
  } finally {
    try {
      await refresh();
    } catch {
      operationError.value ??= message;
    }
    operationInFlight.value = false;
  }
}

function statusNameError(name: string) {
  const length = Array.from(name).length;
  return length < 1 || length > 100 ? 'ステータス名は1〜100文字で入力してください' : null;
}

async function createStatus() {
  const name = newName.value.trim();
  const nameError = statusNameError(name);
  if (nameError) {
    operationError.value = nameError;
    return;
  }
  await run('ステータスを追加できませんでした', async () => {
    await createMutation.mutateAsync({
      params: { path: pathParams.value },
      body: {
        name,
        color: newColor.value,
        position: statuses.value.length,
        is_default: statuses.value.length === 0,
        is_done_state: statuses.value.length === 0,
      },
    });
    newName.value = '';
  });
}

async function saveStatus(status: ProjectStatus) {
  const draft = drafts.value[status.id];
  const name = draft?.name.trim() ?? '';
  const nameError = statusNameError(name);
  if (nameError) {
    operationError.value = nameError;
    return;
  }
  await run('ステータスを更新できませんでした', () =>
    updateMutation.mutateAsync({
      params: { path: { ...pathParams.value, id: status.id } },
      body: { name, color: draft?.color ?? status.color },
    }),
  );
}

async function setUniqueFlag(status: ProjectStatus, flag: 'is_default' | 'is_done_state') {
  if (status[flag]) {
    operationError.value =
      flag === 'is_default'
        ? 'Default は常に1つ必要です。別のステータスを選んでください'
        : 'Done state は常に1つ必要です。別のステータスを選んでください';
    return;
  }

  await run('ステータスの種別を変更できませんでした', async () => {
    // Backend atomically clears the previous unique flag and sets this status.
    await updateMutation.mutateAsync({
      params: { path: { ...pathParams.value, id: status.id } },
      body: { [flag]: true },
    });
  });
}

function deleteBlockReason(status: ProjectStatus) {
  if (statuses.value.length <= 1) return '最後のステータスは削除できません';
  if (status.is_default) return 'Default のステータスは削除できません';
  if (status.is_done_state && statuses.value.filter((item) => item.is_done_state).length <= 1) {
    return '唯一の Done state は削除できません';
  }
  return null;
}

function requestDelete(status: ProjectStatus) {
  const reason = deleteBlockReason(status);
  if (reason) {
    operationError.value = reason;
    return;
  }
  operationError.value = null;
  deleteTarget.value = status;
}

async function confirmDelete() {
  const target = deleteTarget.value;
  if (!target) return;
  const migrationTarget = statuses.value.find((status) => status.is_default);
  await run('ステータスを削除できませんでした', () =>
    deleteMutation.mutateAsync({
      params: {
        path: { ...pathParams.value, id: target.id },
        query: { migrate_to_status_id: migrationTarget?.id },
      },
    }),
  );
  if (!operationError.value) deleteTarget.value = null;
}

function handleDeleteDialogOpen(open: boolean) {
  if (!open && isMutating.value) return;
  if (!open) deleteTarget.value = null;
}

function preventDeleteDialogClose(event: Event) {
  if (isMutating.value) event.preventDefault();
}

async function moveStatus(index: number, offset: -1 | 1) {
  const reordered = [...statuses.value];
  const target = index + offset;
  if (target < 0 || target >= reordered.length) return;
  [reordered[index], reordered[target]] = [reordered[target]!, reordered[index]!];
  await run('並び順を変更できませんでした', () =>
    reorderMutation.mutateAsync({
      params: { path: pathParams.value },
      body: { ids: reordered.map((status) => status.id) },
    }),
  );
}
</script>

<template>
  <section aria-labelledby="workflow-statuses-heading">
    <div class="mb-5 border-b pb-4">
      <h2 id="workflow-statuses-heading" class="text-xl font-semibold">ワークフローステータス</h2>
      <p class="mt-1 text-sm text-muted-foreground">
        タスクの進行段階、色、既定値、完了状態を管理します。
      </p>
    </div>

    <div
      v-if="statusesQuery.isLoading.value"
      role="status"
      class="py-12 text-center text-sm text-muted-foreground"
    >
      ステータスを読み込み中…
    </div>
    <div
      v-else-if="statusesQuery.isError.value"
      role="alert"
      class="rounded-md border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive"
    >
      ステータスを読み込めませんでした
      <Button
        type="button"
        variant="outline"
        size="sm"
        class="ml-3"
        @click="statusesQuery.refetch()"
        >再試行</Button
      >
    </div>
    <div v-else>
      <p
        v-if="operationError"
        role="alert"
        class="mb-4 rounded-md bg-destructive/10 p-3 text-sm text-destructive"
      >
        {{ operationError }}
      </p>

      <div
        v-if="statuses.length === 0"
        class="mb-4 rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground"
      >
        ステータスがありません。最初のステータスを追加してください。
      </div>

      <ul v-else aria-label="ステータス一覧" class="mb-4 space-y-2">
        <li v-for="(status, index) in statuses" :key="status.id" class="rounded-lg border p-3">
          <div class="flex flex-col gap-3 sm:flex-row sm:items-center">
            <div class="flex min-w-0 flex-1 items-center gap-2">
              <Input
                :aria-label="`${status.name}の色`"
                type="color"
                class="h-9 w-12 shrink-0 cursor-pointer p-1"
                :model-value="drafts[status.id]?.color ?? status.color"
                @update:model-value="
                  (value) => {
                    if (drafts[status.id]) drafts[status.id]!.color = String(value);
                  }
                "
              />
              <Input
                :aria-label="`${status.name}の名前`"
                :model-value="drafts[status.id]?.name ?? status.name"
                @update:model-value="
                  (value) => {
                    if (drafts[status.id]) drafts[status.id]!.name = String(value);
                  }
                "
              />
              <Button
                type="button"
                size="sm"
                variant="outline"
                :disabled="isMutating"
                :aria-label="`${status.name}を保存`"
                @click="saveStatus(status)"
              >
                保存
              </Button>
            </div>

            <div class="flex flex-wrap items-center gap-3">
              <label class="flex cursor-pointer items-center gap-1.5 text-xs">
                <Checkbox
                  :model-value="status.is_default"
                  :disabled="isMutating"
                  @update:model-value="setUniqueFlag(status, 'is_default')"
                />
                Default
              </label>
              <label class="flex cursor-pointer items-center gap-1.5 text-xs">
                <Checkbox
                  :model-value="status.is_done_state"
                  :disabled="isMutating"
                  @update:model-value="setUniqueFlag(status, 'is_done_state')"
                />
                Done state
              </label>
              <div class="flex">
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  :disabled="index === 0 || isMutating"
                  :aria-label="`${status.name}を上へ`"
                  @click="moveStatus(Number(index), -1)"
                >
                  <PhArrowUp class="size-4" />
                </Button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  :disabled="index === statuses.length - 1 || isMutating"
                  :aria-label="`${status.name}を下へ`"
                  @click="moveStatus(Number(index), 1)"
                >
                  <PhArrowDown class="size-4" />
                </Button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  class="text-destructive"
                  :disabled="isMutating"
                  :aria-label="`${status.name}を削除`"
                  @click="requestDelete(status)"
                >
                  <PhTrash class="size-4" />
                </Button>
              </div>
            </div>
          </div>
        </li>
      </ul>

      <form class="rounded-lg border border-dashed p-3" @submit.prevent="createStatus">
        <p class="mb-2 text-sm font-medium">ステータスを追加</p>
        <div class="flex flex-col gap-2 sm:flex-row">
          <Input
            v-model="newColor"
            aria-label="新しいステータスの色"
            type="color"
            class="h-9 w-12 shrink-0 cursor-pointer p-1"
          />
          <Input v-model="newName" aria-label="新しいステータス名" placeholder="例: レビュー中" />
          <Button type="submit" :disabled="isMutating"> <PhPlus class="size-4" />追加 </Button>
        </div>
      </form>
    </div>
  </section>

  <Dialog :open="!!deleteTarget" @update:open="handleDeleteDialogOpen">
    <DialogContent
      :show-close-button="false"
      @cancel="preventDeleteDialogClose"
      @escape-key-down="preventDeleteDialogClose"
      @pointer-down-outside="preventDeleteDialogClose"
    >
      <DialogHeader>
        <DialogTitle>ステータスを削除しますか？</DialogTitle>
        <DialogDescription>
          「{{ deleteTarget?.name }}」を削除します。この操作は取り消せません。
        </DialogDescription>
      </DialogHeader>
      <DialogFooter>
        <Button type="button" variant="outline" @click="deleteTarget = null">キャンセル</Button>
        <Button type="button" variant="destructive" :disabled="isMutating" @click="confirmDelete"
          >削除する</Button
        >
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

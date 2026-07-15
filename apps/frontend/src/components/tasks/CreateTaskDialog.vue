<script setup lang="ts">
import { Loader2, X } from '@lucide/vue';
import { computed, ref, watch } from 'vue';
import { useQueryClient } from '@tanstack/vue-query';
import {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui';

import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import type { components } from '@/generated/api';
import { apiClient } from '@/lib/api-vue-query';
import { taskDetailHref } from '@/lib/task-display';
import { toIsoDate } from '@/lib/task-date';

const CREATE_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;

type Priority = components['schemas']['TaskPriority'];
type Status = components['schemas']['ProjectStatusResponse'];
type CreatedTask = components['schemas']['TaskDetailResponse'];

const props = withDefaults(
  defineProps<{
    open: boolean;
    tenantId: string;
    tenantDisplayId: string;
    projectId: string;
    projectKey: string;
    statuses: Status[];
    navigateOnSuccess?: boolean;
  }>(),
  { navigateOnSuccess: true },
);

const emit = defineEmits<{
  'update:open': [value: boolean];
  created: [task: CreatedTask];
}>();

const queryClient = useQueryClient();
const title = ref('');
const statusId = ref('');
const description = ref('');
const softDeadline = ref('');
const hardDeadline = ref('');
const priority = ref<Priority>('Medium');
const validationMessage = ref<string | null>(null);
const requestError = ref<string | null>(null);
const successMessage = ref<string | null>(null);

const defaultStatusId = computed(
  () => props.statuses.find((status) => status.is_default)?.id ?? props.statuses[0]?.id ?? '',
);

const createMutation = apiClient.useMutation('post', CREATE_TASK_PATH);

watch(
  () => [props.open, defaultStatusId.value] as const,
  ([open]) => {
    if (open && !props.statuses.some((status) => status.id === statusId.value)) {
      statusId.value = defaultStatusId.value;
    }
  },
  { immediate: true },
);

function onOpenChange(value: boolean) {
  if (!value && createMutation.isPending.value) return;
  emit('update:open', value);
}

function resetForm() {
  title.value = '';
  statusId.value = defaultStatusId.value;
  description.value = '';
  softDeadline.value = '';
  hardDeadline.value = '';
  priority.value = 'Medium';
  validationMessage.value = null;
  requestError.value = null;
  successMessage.value = null;
}

async function submit() {
  if (createMutation.isPending.value) return;

  const normalizedTitle = title.value.trim();
  validationMessage.value = null;
  requestError.value = null;
  successMessage.value = null;

  if (!normalizedTitle) {
    validationMessage.value = 'タイトルを入力してください';
    return;
  }
  if (!statusId.value) {
    validationMessage.value = 'ステータスを選択してください';
    return;
  }

  const body: components['schemas']['CreateTaskRequest'] = {
    title: normalizedTitle,
    status_id: statusId.value,
    priority: priority.value,
  };
  const normalizedDescription = description.value.trim();
  if (normalizedDescription) body.description = normalizedDescription;
  if (softDeadline.value) body.soft_deadline = toIsoDate(softDeadline.value);
  if (hardDeadline.value) body.hard_deadline = toIsoDate(hardDeadline.value);

  try {
    const created = await createMutation.mutateAsync({
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
      body,
    });
    if (!props.navigateOnSuccess) {
      await queryClient.invalidateQueries({ queryKey: ['get', CREATE_TASK_PATH] });
    }
    emit('created', created);
    resetForm();
    successMessage.value = 'タスクを作成しました';
    if (props.navigateOnSuccess) {
      window.location.assign(
        taskDetailHref(props.tenantDisplayId, props.projectKey, created.seq_id),
      );
    }
  } catch {
    requestError.value = 'タスクの作成に失敗しました。もう一度お試しください';
  }
}
</script>

<template>
  <DialogRoot v-if="open" :open="true" @update:open="onOpenChange">
    <DialogPortal>
      <DialogOverlay class="fixed inset-0 z-50 bg-black/50" />
      <DialogContent
        class="fixed top-1/2 left-1/2 z-50 max-h-[90vh] w-[calc(100%-2rem)] max-w-lg -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-lg border bg-background p-6 shadow-lg"
      >
        <header class="mb-5 flex items-start justify-between gap-4">
          <div>
            <DialogTitle class="text-lg font-semibold">新規タスク</DialogTitle>
            <DialogDescription class="mt-1 text-sm text-muted-foreground">
              {{ projectKey }} にタスクを追加します
            </DialogDescription>
          </div>
          <DialogClose as-child>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="閉じる"
              :disabled="createMutation.isPending.value"
            >
              <X class="size-4" />
            </Button>
          </DialogClose>
        </header>

        <HydrationSafeForm v-slot="{ isHydrated }" class="space-y-4" @submit="submit">
          <label class="block space-y-1.5 text-sm font-medium">
            タイトル <span class="text-destructive">*</span>
            <Input v-model="title" name="title" autocomplete="off" autofocus />
          </label>

          <label class="block space-y-1.5 text-sm font-medium">
            ステータス <span class="text-destructive">*</span>
            <select
              v-model="statusId"
              name="status_id"
              class="flex h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
            >
              <option disabled value="">選択してください</option>
              <option v-for="status in statuses" :key="status.id" :value="status.id">
                {{ status.name }}
              </option>
            </select>
          </label>

          <label class="block space-y-1.5 text-sm font-medium">
            説明
            <Textarea v-model="description" name="description" rows="3" />
          </label>

          <div class="grid gap-4 sm:grid-cols-2">
            <label class="block space-y-1.5 text-sm font-medium">
              期限
              <Input v-model="softDeadline" name="soft_deadline" type="date" />
            </label>
            <label class="block space-y-1.5 text-sm font-medium">
              最終期限
              <Input v-model="hardDeadline" name="hard_deadline" type="date" />
            </label>
          </div>

          <label class="block space-y-1.5 text-sm font-medium">
            優先度
            <select
              v-model="priority"
              name="priority"
              class="flex h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
            >
              <option value="Critical">重大</option>
              <option value="High">高</option>
              <option value="Medium">中</option>
              <option value="Low">低</option>
              <option value="Trivial">些細</option>
            </select>
          </label>

          <p v-if="validationMessage" role="alert" class="text-sm text-destructive">
            {{ validationMessage }}
          </p>
          <p v-if="requestError" role="alert" class="text-sm text-destructive">
            {{ requestError }}
          </p>
          <p v-if="successMessage" role="status" class="text-sm text-emerald-600">
            {{ successMessage }}
          </p>

          <footer class="flex justify-end gap-2 pt-2">
            <DialogClose as-child>
              <Button type="button" variant="outline" :disabled="createMutation.isPending.value">
                キャンセル
              </Button>
            </DialogClose>
            <Button
              type="submit"
              :disabled="createMutation.isPending.value || !isHydrated || !statusId"
            >
              <Loader2 v-if="createMutation.isPending.value" class="mr-2 size-4 animate-spin" />
              {{ createMutation.isPending.value ? '作成中...' : '作成' }}
            </Button>
          </footer>
        </HydrationSafeForm>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

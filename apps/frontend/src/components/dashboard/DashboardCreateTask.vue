<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import type { components } from '@/generated/api';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import {
  fetchClient,
  useAssignableUsersQuery,
  projectLabelsQueryOptions,
} from '@/lib/api-vue-query';
import CreateTaskDialog from '@/components/tasks/CreateTaskDialog.vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

const props = defineProps<{
  open: boolean;
  tenantId: TenantUuid;
  projects: components['schemas']['ProjectResponse'][];
}>();
const emit = defineEmits<{ 'update:open': [open: boolean]; created: [] }>();
const selectedId = ref('');
const editing = ref(false);
const project = computed(() => props.projects.find((p) => p.id === selectedId.value));
const projectId = computed(() => project.value?.id as ProjectUuid | undefined);
const STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
const statuses = useQuery({
  queryKey: computed(
    () =>
      [
        'get',
        STATUSES_PATH,
        {
          params: { path: { tenant_id: props.tenantId, project_id: projectId.value ?? '' } },
        },
      ] as const,
  ),
  queryFn: async ({ queryKey, signal }) => {
    const { data, error } = await fetchClient.GET(STATUSES_PATH, {
      params: queryKey[2].params,
      signal,
    });
    if (error) throw error;
    return data;
  },
  enabled: computed(() => props.open && !!projectId.value),
});
const labels = useQuery(
  computed(() => ({
    ...projectLabelsQueryOptions(props.tenantId, projectId.value),
    enabled: props.open && editing.value && !!projectId.value,
  })),
);
const members = useAssignableUsersQuery(
  () => props.tenantId,
  () => (props.open && editing.value ? projectId.value : null),
);
watch(
  () => props.open,
  (open) => {
    if (open) selectedId.value = props.projects[0]?.id ?? '';
    else {
      editing.value = false;
      selectedId.value = '';
    }
  },
  { immediate: true },
);
function close() {
  emit('update:open', false);
}
</script>

<template>
  <Dialog :open="open && !editing" @update:open="close">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>タスクを作成</DialogTitle>
        <DialogDescription>タスクを追加するプロジェクトを選んでください。</DialogDescription>
      </DialogHeader>
      <label class="grid gap-2 text-sm">
        プロジェクト
        <select v-model="selectedId" class="h-10 rounded-md border bg-background px-3">
          <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </label>
      <div v-if="statuses.isError.value" role="alert" class="text-sm text-destructive">
        ステータスを取得できませんでした。
        <Button variant="outline" size="sm" @click="statuses.refetch()">再試行</Button>
      </div>
      <p
        v-else-if="statuses.isSuccess.value && !statuses.data.value?.length"
        class="text-sm text-muted-foreground"
      >
        このプロジェクトにはステータスがありません。プロジェクト設定で追加してください。
      </p>
      <Button
        :disabled="!project || !statuses.isSuccess.value || !statuses.data.value?.length"
        @click="editing = true"
      >
        {{ statuses.isLoading.value ? '読み込み中…' : '次へ' }}
      </Button>
    </DialogContent>
  </Dialog>
  <CreateTaskDialog
    v-if="project && editing"
    :open="open"
    :tenant-id="tenantId"
    :project-id="project.id"
    :project-key="project.key"
    :statuses="statuses.data.value ?? []"
    :labels="labels.data.value"
    :labels-loading="labels.isLoading.value"
    :labels-error="labels.isError.value && !labels.data.value"
    :members="members.data.value"
    :members-loading="members.isLoading.value"
    :members-error="members.isError.value && !members.data.value"
    @update:open="emit('update:open', $event)"
    @retry-labels="labels.refetch()"
    @retry-members="members.refetch()"
    @created="
      close();
      emit('created');
    "
  />
</template>

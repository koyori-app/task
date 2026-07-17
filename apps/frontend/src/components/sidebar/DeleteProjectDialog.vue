<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { ref, watch } from 'vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { apiClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

const LIST_PROJECTS_PATH = '/v1/tenants/{tenant_id}/projects' as const;
const PROJECT_PATH = '/v1/tenants/{tenant_id}/projects/{id}' as const;

const props = defineProps<{
  open: boolean;
  tenantId: string;
  project: ProjectResponse | null;
}>();

const emit = defineEmits<{
  'update:open': [open: boolean];
  deleted: [project: ProjectResponse];
}>();

const queryClient = useQueryClient();
const deleteError = ref<string | null>(null);

const deleteMutation = apiClient.useMutation('delete', PROJECT_PATH);

watch(
  () => props.open,
  (open) => {
    if (open) deleteError.value = null;
  },
);

function onOpenChange(open: boolean) {
  // 削除リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (!open && deleteMutation.isPending.value) return;
  emit('update:open', open);
}

async function confirmDelete() {
  const target = props.project;
  if (!target) return;
  deleteError.value = null;
  try {
    await deleteMutation.mutateAsync({
      params: { path: { tenant_id: props.tenantId, id: target.id } },
    });
    await queryClient.invalidateQueries({ queryKey: ['get', LIST_PROJECTS_PATH] });
    emit('update:open', false);
    emit('deleted', target);
  } catch {
    deleteError.value = 'プロジェクトを削除できませんでした';
  }
}
</script>

<template>
  <Dialog v-if="open" :open="true" @update:open="onOpenChange">
    <DialogContent class="max-w-md" :show-close-button="false">
      <DialogHeader>
        <DialogTitle>プロジェクトを削除しますか？</DialogTitle>
        <DialogDescription>
          「{{ project?.name }}」を削除します。この操作は取り消せません。
        </DialogDescription>
      </DialogHeader>
      <p v-if="deleteError" role="alert" class="text-sm text-destructive">{{ deleteError }}</p>
      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          :disabled="deleteMutation.isPending.value"
          @click="emit('update:open', false)"
        >
          キャンセル
        </Button>
        <Button
          type="button"
          variant="destructive"
          :disabled="deleteMutation.isPending.value"
          @click="confirmDelete"
        >
          {{ deleteMutation.isPending.value ? '削除中…' : '削除する' }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

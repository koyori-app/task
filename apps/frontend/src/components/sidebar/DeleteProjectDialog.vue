<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import {
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui';
import { ref, watch } from 'vue';
import { Button } from '@/components/ui/button';
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
  <DialogRoot :open="open" @update:open="onOpenChange">
    <DialogPortal>
      <DialogOverlay
        class="fixed inset-0 z-50 bg-black/50"
        data-testid="delete-project-dialog-overlay"
      />
      <DialogContent
        class="fixed left-1/2 top-1/2 z-50 grid w-[calc(100%-2rem)] max-w-md -translate-x-1/2 -translate-y-1/2 gap-4 rounded-lg border bg-background p-6 shadow-lg"
      >
        <div class="space-y-1.5">
          <DialogTitle class="text-lg font-semibold">プロジェクトを削除しますか？</DialogTitle>
          <DialogDescription class="text-sm text-muted-foreground">
            「{{ project?.name }}」を削除します。この操作は取り消せません。
          </DialogDescription>
        </div>
        <p v-if="deleteError" role="alert" class="text-sm text-destructive">{{ deleteError }}</p>
        <div class="flex justify-end gap-2">
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
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

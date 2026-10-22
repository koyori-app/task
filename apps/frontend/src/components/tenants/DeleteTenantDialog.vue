<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { computed, ref, watch } from 'vue';

import { Button, Dialog, Field, Input } from '@koyori-app/ui-vue';
import { apiClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';

type TenantResponse = Pick<components['schemas']['TenantResponse'], 'id' | 'name' | 'display_id'>;

const LIST_TENANTS_PATH = '/v1/tenants' as const;
const TENANT_PATH = '/v1/tenants/{id}' as const;

const props = defineProps<{
  open: boolean;
  tenant: TenantResponse;
}>();

const emit = defineEmits<{
  'update:open': [open: boolean];
  deleted: [tenant: TenantResponse];
}>();

const queryClient = useQueryClient();
const deleteError = ref<string | null>(null);
const confirmText = ref('');

const deleteMutation = apiClient.useMutation('delete', TENANT_PATH);

/** 打ち間違いで消えないよう、URL と同じ文字列を打たせる。 */
const canDelete = computed(() => confirmText.value === props.tenant.display_id);

watch(
  () => props.open,
  (open) => {
    if (open) {
      deleteError.value = null;
      confirmText.value = '';
    }
  },
);

function onClose() {
  // 削除の最中は閉じない（結果を見逃さないため）
  if (deleteMutation.isPending.value) return;
  emit('update:open', false);
}

async function confirmDelete() {
  if (!canDelete.value) return;
  deleteError.value = null;
  try {
    await deleteMutation.mutateAsync({ params: { path: { id: props.tenant.id } } });
    await queryClient.invalidateQueries({ queryKey: ['get', LIST_TENANTS_PATH] });
    emit('update:open', false);
    emit('deleted', props.tenant);
  } catch {
    deleteError.value = 'テナントを削除できませんでした';
  }
}
</script>

<template>
  <div class="contents [--koyori-dialog-width:440px]">
    <Dialog
      :open="open"
      :title="`「${tenant.name}」を削除しますか？`"
      description="この操作は取り消せません。このテナントのプロジェクト・タスク・ラベル・招待がすべて完全に削除されます。"
      :on-close="onClose"
    >
      <div class="grid gap-4">
        <Field
          id="delete-tenant-confirm"
          :label="`確認のため ${tenant.display_id} と入力してください`"
        >
          <Input
            class="[&_input]:font-mono"
            :placeholder="tenant.display_id"
            :value="confirmText"
            :on-value-change="(v: string) => (confirmText = v)"
          />
        </Field>

        <p v-if="deleteError" role="alert" class="text-sm text-destructive">{{ deleteError }}</p>
      </div>

      <template #actions>
        <Button
          variant="tertiary"
          :disabled="deleteMutation.isPending.value"
          label="キャンセル"
          @click="onClose"
        />
        <Button
          variant="danger"
          :disabled="!canDelete || deleteMutation.isPending.value"
          label="テナントを削除"
          @click="confirmDelete"
        />
      </template>
    </Dialog>
  </div>
</template>

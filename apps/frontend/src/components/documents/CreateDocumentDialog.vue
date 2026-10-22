<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { ref, watch } from 'vue';

import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { Button, Spinner } from '@koyori-app/ui-vue';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { components } from '@/generated/api';
import { DRIVE_FILES_PATH, fetchClient } from '@/lib/api-vue-query';
import { documentFileName, MAX_DOCUMENT_TITLE_LENGTH } from '@/lib/documents/name';
import { DOCUMENT_TEMPLATES } from '@/lib/documents/templates';

type DriveFile = components['schemas']['DriveFileResponse'];

const props = defineProps<{
  open: boolean;
  tenantId: string;
  /** プロジェクトのルートフォルダ（ドキュメント置き場） */
  folderId: string;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  created: [file: DriveFile];
}>();

const queryClient = useQueryClient();
const title = ref('');
const templateId = ref(DOCUMENT_TEMPLATES[0]!.id);
const pending = ref(false);
const errorMessage = ref<string | null>(null);

watch(
  () => props.open,
  (open) => {
    if (open) return;
    title.value = '';
    templateId.value = DOCUMENT_TEMPLATES[0]!.id;
    errorMessage.value = null;
  },
);

function onOpenChange(value: boolean) {
  if (!value && pending.value) return;
  emit('update:open', value);
}

async function submit() {
  if (pending.value) return;
  const normalized = title.value.trim();
  errorMessage.value = null;
  if (!normalized) {
    errorMessage.value = 'タイトルを入力してください';
    return;
  }
  if (Array.from(normalized).length > MAX_DOCUMENT_TITLE_LENGTH) {
    errorMessage.value = `タイトルは ${MAX_DOCUMENT_TITLE_LENGTH} 文字以内で入力してください`;
    return;
  }
  const template = DOCUMENT_TEMPLATES.find((item) => item.id === templateId.value);
  const name = documentFileName(normalized);

  pending.value = true;
  try {
    // ハンドラは file より前の name / folder_id しか読まない（Drive 仕様 §8.1）
    const form = new FormData();
    form.append('folder_id', props.folderId);
    form.append('name', name);
    form.append(
      'file',
      new File([template?.body(normalized) ?? ''], name, { type: 'text/markdown' }),
    );
    const { data, error, response } = await fetchClient.POST(DRIVE_FILES_PATH, {
      params: { path: { tenant_id: props.tenantId } },
      body: form,
    });
    if (error) {
      errorMessage.value =
        response.status === 413
          ? '保存容量の上限を超えています'
          : 'ドキュメントを作成できませんでした';
      return;
    }
    void queryClient.invalidateQueries({ queryKey: ['get', DRIVE_FILES_PATH] });
    emit('created', data);
  } catch {
    // オフライン・切断で fetch が reject した。入力は残す
    errorMessage.value = 'ドキュメントを作成できませんでした';
  } finally {
    pending.value = false;
  }
}
</script>

<template>
  <Dialog v-if="open" :open="true" @update:open="onOpenChange">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>新しいドキュメント</DialogTitle>
        <DialogDescription
          >種類は本文のテンプレートを選ぶだけで、保存はされません</DialogDescription
        >
      </DialogHeader>
      <HydrationSafeForm v-slot="{ isHydrated }" class="flex flex-col gap-4" @submit="submit">
        <div class="space-y-1.5">
          <Label for="document-title">タイトル</Label>
          <Input
            id="document-title"
            v-model="title"
            name="title"
            autocomplete="off"
            autofocus
            required
            :maxlength="MAX_DOCUMENT_TITLE_LENGTH"
          />
        </div>
        <div class="space-y-1.5">
          <Label for="document-template">種類</Label>
          <Select v-model="templateId">
            <SelectTrigger id="document-template" class="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="item in DOCUMENT_TEMPLATES" :key="item.id" :value="item.id">
                {{ item.label }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
        <p v-if="errorMessage" role="alert" class="text-sm text-destructive">
          {{ errorMessage }}
        </p>
        <DialogFooter>
          <Button
            type="button"
            variant="tertiary"
            :disabled="pending"
            label="取り消す"
            @click="onOpenChange(false)"
          />
          <Button
            type="submit"
            :disabled="pending || !isHydrated"
            :label="pending ? '作成中…' : '作成'"
          >
            <template #icon><Spinner v-if="pending" hide-label /></template>
          </Button>
        </DialogFooter>
      </HydrationSafeForm>
    </DialogContent>
  </Dialog>
</template>

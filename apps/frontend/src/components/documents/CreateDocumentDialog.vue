<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { ref, watch } from 'vue';

import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { Button, Dialog, Field, Input, Picker, Spinner } from '@koyori-app/ui-vue';
import { useHydrated } from '@/composables/useHydrated';
import type { components } from '@/generated/api';
import { DRIVE_FILES_PATH, fetchClient } from '@/lib/api-vue-query';
import { documentFileName, MAX_DOCUMENT_TITLE_LENGTH } from '@/lib/documents/name';
import { DOCUMENT_TEMPLATES } from '@/lib/documents/templates';

type DriveFile = components['schemas']['DriveFileResponse'];

const TEMPLATE_ITEMS = DOCUMENT_TEMPLATES.map((item) => ({ value: item.id, label: item.label }));

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
// 送信ボタンはフォームの外（#actions）にあるので、HydrationSafeForm のスロットの値は使えない
const isHydrated = useHydrated();

watch(
  () => props.open,
  (open) => {
    if (open) return;
    title.value = '';
    templateId.value = DOCUMENT_TEMPLATES[0]!.id;
    errorMessage.value = null;
  },
);

function onClose() {
  if (pending.value) return;
  emit('update:open', false);
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
  <div class="contents [--koyori-dialog-width:28rem]">
    <Dialog
      :open="open"
      title="新しいドキュメント"
      description="種類は本文のテンプレートを選ぶだけで、保存はされません"
      :on-close="onClose"
    >
      <HydrationSafeForm id="create-document-form" class="flex flex-col gap-4" @submit="submit">
        <!-- 文字数の上限は送信時に検証する -->
        <Field id="document-title" label="タイトル" required>
          <Input
            name="title"
            autocomplete="off"
            :value="title"
            :on-value-change="(v: string) => (title = v)"
          />
        </Field>
        <Field id="document-template" label="種類">
          <Picker
            label="種類"
            :searchable="false"
            :items="TEMPLATE_ITEMS"
            :selected-values="[templateId]"
            :on-selection-change="(values: string[]) => (templateId = values[0]!)"
          />
        </Field>
        <p v-if="errorMessage" role="alert" class="text-sm text-destructive">
          {{ errorMessage }}
        </p>
      </HydrationSafeForm>
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="pending"
          label="取り消す"
          @click="onClose"
        />
        <Button
          type="submit"
          form="create-document-form"
          :disabled="pending || !isHydrated"
          :label="pending ? '作成中…' : '作成'"
        >
          <template #icon><Spinner v-if="pending" hide-label /></template>
        </Button>
      </template>
    </Dialog>
  </div>
</template>

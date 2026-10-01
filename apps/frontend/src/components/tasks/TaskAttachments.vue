<script setup lang="ts">
import { Download, Paperclip, X } from '@lucide/vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { computed, onBeforeUnmount, ref, watch } from 'vue';

import { Button } from '@/components/ui/button';
import type { components } from '@/generated/api';
import { fetchClient } from '@/lib/api-vue-query';

const ATTACHMENTS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments' as const;
const ATTACHMENT_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments/{attachment_id}' as const;
const FOLDERS_PATH = '/v1/tenants/{tenant_id}/drive/folders' as const;
const FILES_PATH = '/v1/tenants/{tenant_id}/drive/files' as const;
const apiBase = (import.meta.env.VITE_API_BASE ?? '/api').replace(/\/$/, '');
const MAX_PREVIEW_BYTES = 25 * 1024 * 1024;
const MAX_TEXT_PREVIEW_BYTES = 1024 * 1024;
const IMAGE_TYPES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/avif']);

type Attachment = components['schemas']['TaskAttachmentResponse'];
type PreviewMode = 'image' | 'pdf' | 'text';

const props = defineProps<{
  tenantId: string | null;
  projectId: string | null;
  taskId: string;
}>();

const queryClient = useQueryClient();
const input = ref<HTMLInputElement | null>(null);
const removingId = ref<string | null>(null);
type TaskRef = { tenantId: string; projectId: string; taskId: string };
type PendingFile = TaskRef & { id: string; name: string };
const taskKey = (task: TaskRef) => `${task.tenantId}/${task.projectId}/${task.taskId}`;
const currentKey = computed(() => `${props.tenantId}/${props.projectId}/${props.taskId}`);
// アップロード中・失敗・紐付け待ちは、始めたタスクのものとして持つ。
// 分割表示でタスクを切り替えても、別のタスクの進行や失敗を今のタスクの欄に出さない
const uploadingKeys = ref(new Set<string>());
const uploading = computed(() => uploadingKeys.value.has(currentKey.value));
const error = ref<{ key: string; message: string } | null>(null);
const errorMessage = computed(() =>
  error.value?.key === currentKey.value ? error.value.message : null,
);
function setError(key: string, message: string | null) {
  if (message !== null) error.value = { key, message };
  else if (error.value?.key === key) error.value = null;
}
// アップロード済みで紐付けに失敗したファイル。別のタスクへ切り替えても捨てない。
// 元のタスクへ戻れば、捕まえた ID で紐付けを再試行できる
const pendingFiles = ref<PendingFile[]>([]);
const pendingForTask = computed(
  () => pendingFiles.value.find((file) => taskKey(file) === currentKey.value) ?? null,
);
const previewDialog = ref<HTMLDialogElement | null>(null);
const previewFile = ref<Attachment | null>(null);
const previewMode = ref<PreviewMode | null>(null);
const previewUrl = ref<string | null>(null);
const previewText = ref('');
const previewLoading = ref(false);
const previewError = ref<string | null>(null);
let previewController: AbortController | null = null;

function previewType(file: Attachment): PreviewMode | null {
  const mime = file.mime_type.toLowerCase().split(';')[0]?.trim();
  if (file.size > MAX_PREVIEW_BYTES) return null;
  if (mime && IMAGE_TYPES.has(mime)) return 'image';
  if (mime === 'application/pdf') return 'pdf';
  if (
    file.size <= MAX_TEXT_PREVIEW_BYTES &&
    (mime?.startsWith('text/') || mime === 'application/json')
  ) {
    return 'text';
  }
  return null;
}

function clearPreview() {
  previewController?.abort();
  previewController = null;
  if (previewUrl.value) URL.revokeObjectURL(previewUrl.value);
  previewUrl.value = null;
  previewFile.value = null;
  previewMode.value = null;
  previewText.value = '';
  previewError.value = null;
  previewLoading.value = false;
}

function closePreview() {
  previewDialog.value?.close();
  clearPreview();
}

async function openPreview(file: Attachment) {
  const mode = previewType(file);
  if (!mode) return;
  clearPreview();
  previewFile.value = file;
  previewMode.value = mode;
  previewLoading.value = true;
  previewDialog.value?.showModal();
  const controller = new AbortController();
  previewController = controller;
  try {
    const response = await fetch(`${apiBase}${file.url}`, {
      credentials: 'include',
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(
        response.status === 403
          ? 'このファイルを表示する権限がありません'
          : response.status === 404
            ? 'ファイルが見つかりません'
            : 'プレビューを読み込めませんでした',
      );
    }
    const blob = await response.blob();
    if (controller.signal.aborted) return;
    if (blob.size > MAX_PREVIEW_BYTES || (mode === 'text' && blob.size > MAX_TEXT_PREVIEW_BYTES)) {
      throw new Error('ファイルがプレビューのサイズ上限を超えています');
    }
    if (mode === 'pdf' && (await blob.slice(0, 5).text()) !== '%PDF-') {
      throw new Error('PDF ファイルとして表示できません');
    }
    if (controller.signal.aborted) return;
    if (mode === 'text') {
      const content = await blob.text();
      if (controller.signal.aborted) return;
      previewText.value = content;
    } else {
      previewUrl.value = URL.createObjectURL(
        new Blob([blob], {
          type:
            mode === 'pdf' ? 'application/pdf' : file.mime_type.toLowerCase().split(';')[0]?.trim(),
        }),
      );
    }
  } catch (error) {
    if (!controller.signal.aborted) {
      previewError.value =
        error instanceof Error ? error.message : 'プレビューを読み込めませんでした';
    }
  } finally {
    if (!controller.signal.aborted) previewLoading.value = false;
  }
}

onBeforeUnmount(clearPreview);

const queryKey = computed(() => [
  'get',
  ATTACHMENTS_PATH,
  { tenantId: props.tenantId, projectId: props.projectId, taskId: props.taskId },
]);

const attachmentsQuery = useQuery({
  queryKey,
  queryFn: async ({ signal }) => {
    const { data, error } = await fetchClient.GET(ATTACHMENTS_PATH, {
      params: {
        path: { tenant_id: props.tenantId!, project_id: props.projectId!, id: props.taskId },
      },
      signal,
    });
    if (error) throw error;
    return data.attachments;
  },
  enabled: computed(() => !!props.tenantId && !!props.projectId && !!props.taskId),
  retry: false,
});

watch(currentKey, closePreview);

async function attach(file: PendingFile) {
  const path = { tenant_id: file.tenantId, project_id: file.projectId, id: file.taskId };
  const { error } = await fetchClient.POST(ATTACHMENTS_PATH, {
    params: { path },
    body: { drive_file_id: file.id },
  });
  if (error) {
    // 応答だけ失われた場合は再試行時に 409 になる。実際の紐付け状態を確認する。
    const current = await fetchClient.GET(ATTACHMENTS_PATH, { params: { path } });
    if (current.error || !current.data.attachments.some((item) => item.drive_file_id === file.id)) {
      throw new Error('ファイルをタスクに紐付けできませんでした');
    }
  }
  await queryClient.invalidateQueries({
    queryKey: [
      'get',
      ATTACHMENTS_PATH,
      { tenantId: file.tenantId, projectId: file.projectId, taskId: file.taskId },
    ],
  });
  pendingFiles.value = pendingFiles.value.filter((pending) => pending.id !== file.id);
}

async function onFileSelected(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0];
  (event.target as HTMLInputElement).value = '';
  if (
    !file ||
    !props.tenantId ||
    !props.projectId ||
    !props.taskId ||
    uploading.value ||
    pendingForTask.value
  )
    return;
  const target = { tenantId: props.tenantId, projectId: props.projectId, taskId: props.taskId };
  const key = taskKey(target);
  if (file.size === 0) {
    setError(key, '空のファイルは添付できません');
    return;
  }

  uploadingKeys.value.add(key);
  setError(key, null);
  try {
    const folders = await fetchClient.GET(FOLDERS_PATH, {
      params: { path: { tenant_id: target.tenantId } },
    });
    if (folders.error) throw new Error('保存先を読み込めませんでした');
    const projectFolder = folders.data.find(
      (folder) => folder.project_id === target.projectId && folder.parent_id == null,
    );
    if (!projectFolder) throw new Error('プロジェクトの保存先が見つかりません');

    const form = new FormData();
    form.append('folder_id', projectFolder.id);
    form.append('file', file);
    const uploaded = await fetchClient.POST(FILES_PATH, {
      params: { path: { tenant_id: target.tenantId } },
      body: form,
    });
    if (uploaded.error) {
      throw new Error(
        uploaded.response.status === 413
          ? 'ファイルがサイズ上限または保存容量を超えています'
          : 'ファイルをアップロードできませんでした',
      );
    }

    const saved = { ...target, id: uploaded.data.id, name: file.name };
    pendingFiles.value = [...pendingFiles.value, saved];
    await attach(saved);
  } catch (error) {
    setError(key, error instanceof Error ? error.message : 'ファイルを添付できませんでした');
  } finally {
    uploadingKeys.value.delete(key);
  }
}

async function retryAttach() {
  const file = pendingForTask.value;
  if (!file || uploading.value) return;
  const key = taskKey(file);
  uploadingKeys.value.add(key);
  setError(key, null);
  try {
    await attach(file);
  } catch {
    setError(key, 'ファイルをタスクに紐付けできませんでした');
  } finally {
    uploadingKeys.value.delete(key);
  }
}

async function removeAttachment(attachmentId: string) {
  if (!props.tenantId || !props.projectId || removingId.value) return;
  removingId.value = attachmentId;
  const owner = currentKey.value;
  setError(owner, null);
  const key = queryKey.value;
  try {
    const { error, response } = await fetchClient.DELETE(ATTACHMENT_PATH, {
      params: {
        path: {
          tenant_id: props.tenantId,
          project_id: props.projectId,
          id: props.taskId,
          attachment_id: attachmentId,
        },
      },
    });
    if (error) {
      throw new Error(
        response.status === 403
          ? '添付を解除できませんでした。作成者またはテナントオーナーのみ解除できます'
          : '添付を解除できませんでした',
      );
    }
    await queryClient.invalidateQueries({ queryKey: key });
  } catch (error) {
    setError(owner, error instanceof Error ? error.message : '添付を解除できませんでした');
  } finally {
    removingId.value = null;
  }
}
</script>

<template>
  <section class="space-y-2" aria-label="添付ファイル">
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-sm font-semibold">添付ファイル</h2>
      <Button
        type="button"
        variant="outline"
        size="sm"
        :disabled="uploading || !!pendingForTask"
        @click="input?.click()"
      >
        <Paperclip class="mr-1 size-4" aria-hidden="true" />
        {{ uploading ? 'アップロード中…' : 'ファイルを添付' }}
      </Button>
      <input
        ref="input"
        type="file"
        class="sr-only"
        aria-label="添付するファイルを選択"
        @change="onFileSelected"
      />
    </div>

    <p v-if="errorMessage" role="alert" class="text-sm text-destructive">{{ errorMessage }}</p>
    <div v-if="pendingForTask" class="flex items-center gap-2 text-sm">
      <span>{{ pendingForTask.name }} はアップロード済みです</span>
      <Button type="button" variant="outline" size="sm" :disabled="uploading" @click="retryAttach"
        >紐付けを再試行</Button
      >
    </div>

    <p v-if="attachmentsQuery.isLoading.value" class="text-sm text-muted-foreground">読み込み中…</p>
    <div
      v-else-if="attachmentsQuery.isError.value"
      class="flex items-center gap-2 text-sm text-destructive"
    >
      添付ファイルを読み込めませんでした
      <Button type="button" variant="outline" size="sm" @click="attachmentsQuery.refetch()"
        >再試行</Button
      >
    </div>
    <ul v-else-if="attachmentsQuery.data.value?.length" class="space-y-1">
      <li
        v-for="attachment in attachmentsQuery.data.value"
        :key="attachment.id"
        class="flex items-center gap-2 text-sm"
      >
        <button
          v-if="previewType(attachment)"
          type="button"
          class="min-w-0 flex-1 truncate text-left text-primary underline"
          :title="`${attachment.name} をプレビュー`"
          @click="openPreview(attachment)"
        >
          {{ attachment.name }}
        </button>
        <a
          v-else
          :href="`${apiBase}${attachment.url}`"
          class="min-w-0 flex-1 truncate text-primary underline"
          :title="attachment.name"
          >{{ attachment.name }}</a
        >
        <span class="shrink-0 text-xs text-muted-foreground"
          >{{ Math.ceil(attachment.size / 1024) }} KB</span
        >
        <a
          :href="`${apiBase}${attachment.url}`"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded hover:bg-muted"
          :aria-label="`${attachment.name} をダウンロード`"
        >
          <Download class="size-4" aria-hidden="true" />
        </a>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          class="size-7 shrink-0"
          :aria-label="`${attachment.name} の添付を解除`"
          :disabled="removingId === attachment.id"
          @click="removeAttachment(attachment.id)"
        >
          <X class="size-4" aria-hidden="true" />
        </Button>
      </li>
    </ul>
    <p v-else class="text-sm text-muted-foreground">添付ファイルはありません</p>

    <dialog
      ref="previewDialog"
      class="fixed inset-0 m-auto max-h-[90vh] w-[min(90vw,70rem)] rounded-lg border bg-background p-4 shadow-xl backdrop:bg-black/60"
      :aria-label="previewFile ? `${previewFile.name} のプレビュー` : 'ファイルのプレビュー'"
      @close="clearPreview"
    >
      <div class="mb-3 flex items-center justify-between gap-3">
        <h3 class="min-w-0 truncate font-semibold">{{ previewFile?.name }}</h3>
        <div class="flex shrink-0 items-center gap-2">
          <a
            v-if="previewFile"
            :href="`${apiBase}${previewFile.url}`"
            class="text-sm text-primary underline"
            >ダウンロード</a
          >
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label="プレビューを閉じる"
            @click="closePreview"
          >
            <X class="size-4" aria-hidden="true" />
          </Button>
        </div>
      </div>
      <p v-if="previewLoading" class="py-12 text-center text-sm text-muted-foreground">
        読み込み中…
      </p>
      <p v-else-if="previewError" role="alert" class="py-12 text-center text-sm text-destructive">
        {{ previewError }}
      </p>
      <img
        v-else-if="previewMode === 'image' && previewUrl"
        :src="previewUrl"
        :alt="previewFile?.name"
        class="mx-auto max-h-[75vh] max-w-full object-contain"
      />
      <iframe
        v-else-if="previewMode === 'pdf' && previewUrl"
        :src="previewUrl"
        :title="previewFile?.name"
        sandbox=""
        class="h-[75vh] w-full"
      />
      <pre
        v-else-if="previewMode === 'text'"
        class="max-h-[75vh] overflow-auto whitespace-pre-wrap break-words rounded bg-muted p-3 text-sm"
        >{{ previewText }}</pre>
    </dialog>
  </section>
</template>

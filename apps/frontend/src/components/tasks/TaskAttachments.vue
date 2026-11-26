<script setup lang="ts">
import { Download, File as FileIcon, FileImage, FileText, X } from '@lucide/vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { computed, onBeforeUnmount, ref, watch } from 'vue';

import { Button, Dialog } from '@koyori-app/ui-vue';
import UserAvatar from '@/components/UserAvatar.vue';
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
  /** ログイン中ユーザーの ID（/v1/auth/me）。未取得なら null */
  currentUserId?: string | null;
  /** 見ている人がテナントオーナーか。他人が付けた添付の解除を出すかに使う */
  isTenantOwner?: boolean;
  /** 投稿者のアバターを引く相手（プロジェクトのメンバー）。見つからない投稿者は出さない */
  members?: { id: string; username: string; avatar_url?: string | null }[];
}>();

const queryClient = useQueryClient();
const input = ref<HTMLInputElement | null>(null);
const removingId = ref<string | null>(null);
// 解除の確認待ちの添付。確認で「解除する」を押すまで DELETE は送らない
const detachTarget = ref<Attachment | null>(null);

/** 解除できるのは添付の作成者とテナントオーナー（backend の detach と同じ）。作成者の分からない行はオーナーだけ */
function canDetach(attachment: Attachment) {
  return (
    !!props.isTenantOwner ||
    (!!props.currentUserId && attachment.created_by === props.currentUserId)
  );
}

// 解除の応答を待っているのが、いま確認している添付か。別の添付（切り替える前のタスクのもの）の
// 応答待ちなら、この確認は閉じられる。解除は同時に 1 件だけなので、「解除する」は塞いだままにする
const isDetachingTarget = computed(
  () => !!removingId.value && removingId.value === detachTarget.value?.id,
);

function closeDetachConfirm() {
  // 解除リクエストの進行中は閉じない（結果の見逃し防止）
  if (isDetachingTarget.value) return;
  detachTarget.value = null;
}

async function confirmDetach() {
  const target = detachTarget.value;
  if (!target) return;
  await removeAttachment(target.id);
  // 解除を待つ間に別の添付の確認へ移っていたら、そちらの確認は閉じない
  if (detachTarget.value === target) detachTarget.value = null;
}
type TaskRef = { tenantId: string; projectId: string; taskId: string };
type PendingFile = TaskRef & { id: string; name: string };
const taskKey = (task: TaskRef) => `${task.tenantId}/${task.projectId}/${task.taskId}`;
const currentKey = computed(() => `${props.tenantId}/${props.projectId}/${props.taskId}`);
// アップロード中・失敗・紐付け待ちは、始めたタスクのものとして持つ。
// 分割表示でタスクを切り替えても、別のタスクの進行や失敗を今のタスクの欄に出さない
const uploadingKeys = ref(new Set<string>());
const uploading = computed(() => uploadingKeys.value.has(currentKey.value));
const errors = ref(new Map<string, string>());
const errorMessage = computed(() => errors.value.get(currentKey.value) ?? null);
function setError(key: string, message: string | null) {
  if (message !== null) errors.value.set(key, message);
  else errors.value.delete(key);
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

/**
 * プレビューに出すファイルの中身。大きさと PDF の先頭を確かめてから返す。
 * プレビューのダイアログとタイルのサムネイルが同じ取得を使う。
 */
async function fetchPreviewBlob(
  file: Attachment,
  mode: PreviewMode,
  signal: AbortSignal,
): Promise<Blob> {
  const response = await fetch(`${apiBase}${file.url}`, { credentials: 'include', signal });
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
  if (blob.size > MAX_PREVIEW_BYTES || (mode === 'text' && blob.size > MAX_TEXT_PREVIEW_BYTES)) {
    throw new Error('ファイルがプレビューのサイズ上限を超えています');
  }
  if (mode === 'pdf' && (await blob.slice(0, 5).text()) !== '%PDF-') {
    throw new Error('PDF ファイルとして表示できません');
  }
  return blob;
}

function imageBlobUrl(file: Attachment, blob: Blob): string {
  return URL.createObjectURL(
    new Blob([blob], { type: file.mime_type.toLowerCase().split(';')[0]?.trim() }),
  );
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
    const blob = await fetchPreviewBlob(file, mode, controller.signal);
    if (controller.signal.aborted) return;
    if (mode === 'text') {
      const content = await blob.text();
      if (controller.signal.aborted) return;
      previewText.value = content;
    } else {
      previewUrl.value =
        mode === 'pdf'
          ? URL.createObjectURL(new Blob([blob], { type: 'application/pdf' }))
          : imageBlobUrl(file, blob);
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

watch(currentKey, () => {
  closePreview();
  detachTarget.value = null;
});

// ---- タイルのサムネイル ----
// object URL の持ち主はここ。添付 id ごとに 1 つ作り、一覧から消えた・タスクが変わった・unmount の
// どれでも revoke する。取得は添付 id ごとに 1 本で、失敗しても取り直さない（種類のアイコンに倒す）。
const thumbnails = ref(new Map<string, string>());
/** 取得中・取得済み・失敗の添付 id。一覧の再取得で同じ id を取り直さない */
const thumbnailRequests = new Map<string, AbortController | null>();

function dropThumbnail(id: string) {
  thumbnailRequests.get(id)?.abort();
  thumbnailRequests.delete(id);
  const url = thumbnails.value.get(id);
  if (url) URL.revokeObjectURL(url);
  thumbnails.value.delete(id);
}

function clearThumbnails() {
  // Map は走査中に消しても安全
  for (const id of thumbnailRequests.keys()) dropThumbnail(id);
}

async function loadThumbnail(file: Attachment) {
  const controller = new AbortController();
  thumbnailRequests.set(file.id, controller);
  // 取得を始めたタスク。応答が届くまでに別のタスクへ移っていたら捨てる
  const owner = currentKey.value;
  try {
    const blob = await fetchPreviewBlob(file, 'image', controller.signal);
    if (controller.signal.aborted || owner !== currentKey.value) return;
    thumbnails.value.set(file.id, imageBlobUrl(file, blob));
  } catch {
    // 失敗は種類のアイコンのまま。id は残して取り直さない
  } finally {
    if (thumbnailRequests.get(file.id) === controller) thumbnailRequests.set(file.id, null);
  }
}

watch(
  [currentKey, () => attachmentsQuery.data.value],
  ([key, attachments], [previousKey]) => {
    if (key !== previousKey) clearThumbnails();
    const listed = new Set((attachments ?? []).map((attachment) => attachment.id));
    for (const id of thumbnailRequests.keys()) {
      if (!listed.has(id)) dropThumbnail(id);
    }
    for (const attachment of attachments ?? []) {
      if (previewType(attachment) === 'image' && !thumbnailRequests.has(attachment.id)) {
        void loadThumbnail(attachment);
      }
    }
  },
  { immediate: true },
);
onBeforeUnmount(clearThumbnails);

const membersById = computed(() => new Map((props.members ?? []).map((m) => [m.id, m])));
function uploader(attachment: Attachment) {
  return attachment.created_by ? (membersById.value.get(attachment.created_by) ?? null) : null;
}

/** 種類のアイコンの下に出す拡張子（無ければ出さない） */
function extension(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toLowerCase() : '';
}

function tileIcon(file: Attachment) {
  const mime = file.mime_type.toLowerCase();
  if (mime.startsWith('image/')) return FileImage;
  if (mime.startsWith('text/') || mime === 'application/pdf' || mime === 'application/json') {
    return FileText;
  }
  return FileIcon;
}

/** 説明の見出しの 📎 から呼ぶ。アップロード中と紐付け待ちの間は開かない */
function openPicker() {
  if (uploading.value || pendingForTask.value) return;
  input.value?.click();
}
defineExpose({ openPicker, uploading });

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
          ? '添付を解除できるのは、添付した人とテナントオーナーだけです。'
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
  <section aria-label="添付ファイル" data-task-attachments>
    <!--
      見出しとボタンの節は持たない。ファイルを選ぶ入口は説明の見出しの 📎（消費側）で、ここは
      添付があるときだけタイルを並べる。0 件と読み込み中は何も描かない。
    -->
    <input
      ref="input"
      type="file"
      class="sr-only"
      aria-label="添付するファイルを選択"
      @change="onFileSelected"
    />

    <p v-if="errorMessage" role="alert" class="mt-3 text-sm text-destructive">
      {{ errorMessage }}
    </p>
    <div v-if="pendingForTask" class="mt-3 flex items-center gap-2 text-sm">
      <span>{{ pendingForTask.name }} はアップロード済みです</span>
      <Button
        type="button"
        variant="tertiary"
        :disabled="uploading"
        label="紐付けを再試行"
        @click="retryAttach"
      />
    </div>

    <div
      v-if="attachmentsQuery.isError.value"
      class="mt-3 flex items-center gap-2 text-sm text-destructive"
    >
      添付ファイルを読み込めませんでした
      <Button type="button" variant="tertiary" label="再試行" @click="attachmentsQuery.refetch()" />
    </div>
    <ul
      v-else-if="attachmentsQuery.data.value?.length"
      class="mt-3 flex flex-wrap gap-3"
      data-task-attachment-tiles
    >
      <li
        v-for="attachment in attachmentsQuery.data.value"
        :key="attachment.id"
        class="group relative h-[152px] w-[164px] shrink-0 rounded-md bg-[var(--koyori-color-surface-hover)] p-2"
        data-task-attachment-tile
      >
        <!-- 押すとプレビュー。プレビューできない種類はダウンロードになる -->
        <component
          :is="previewType(attachment) ? 'button' : 'a'"
          :type="previewType(attachment) ? 'button' : undefined"
          :href="previewType(attachment) ? undefined : `${apiBase}${attachment.url}`"
          class="flex h-[104px] w-full flex-col items-center justify-center gap-1 overflow-hidden rounded text-muted-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2"
          :title="previewType(attachment) ? `${attachment.name} をプレビュー` : attachment.name"
          :aria-label="
            previewType(attachment) ? `${attachment.name} をプレビュー` : attachment.name
          "
          data-task-attachment-open
          @click="previewType(attachment) && openPreview(attachment)"
        >
          <img
            v-if="thumbnails.get(attachment.id)"
            :src="thumbnails.get(attachment.id)"
            alt=""
            class="max-h-full max-w-full rounded-sm object-contain shadow"
            data-task-attachment-thumbnail
          />
          <template v-else>
            <component :is="tileIcon(attachment)" class="size-8" aria-hidden="true" />
            <span v-if="extension(attachment.name)" class="text-xs uppercase">
              {{ extension(attachment.name) }}
            </span>
          </template>
        </component>
        <div class="mt-2 flex h-6 items-center gap-2">
          <span class="min-w-0 flex-1 truncate text-xs" :title="attachment.name">
            {{ attachment.name }}
          </span>
          <UserAvatar
            v-if="uploader(attachment)"
            :user="uploader(attachment)!"
            size="sm"
            class="shrink-0 border bg-white"
          />
        </div>
        <!-- ダウンロードと解除は、タイルへのホバーとフォーカスで出す。隠す間もフォーカスは届くよう opacity で消す -->
        <div
          class="absolute top-1 right-1 flex gap-1 rounded-md bg-background/90 opacity-0 shadow-sm transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"
        >
          <a
            :href="`${apiBase}${attachment.url}`"
            class="inline-flex size-7 shrink-0 items-center justify-center rounded-md hover:bg-muted"
            :aria-label="`${attachment.name} をダウンロード`"
          >
            <Download class="size-4" aria-hidden="true" />
          </a>
          <Button
            v-if="canDetach(attachment)"
            type="button"
            variant="ghost"
            class="shrink-0"
            :ariaLabel="`${attachment.name} の添付を解除`"
            :disabled="removingId === attachment.id"
            @click="detachTarget = attachment"
          >
            <template #icon><X class="size-4" aria-hidden="true" /></template>
          </Button>
        </div>
      </li>
    </ul>

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
            class="text-sm text-link underline"
            >ダウンロード</a
          >
          <Button
            type="button"
            variant="ghost"
            ariaLabel="プレビューを閉じる"
            @click="closePreview"
          >
            <template #icon><X class="size-4" aria-hidden="true" /></template>
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

    <Dialog
      :open="!!detachTarget"
      title="添付を解除しますか？"
      :description="`「${detachTarget?.name}」をこのタスクから外します。ファイルは Drive に残ります。`"
      :on-close="closeDetachConfirm"
    >
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="isDetachingTarget"
          label="キャンセル"
          @click="closeDetachConfirm"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="!!removingId"
          :label="isDetachingTarget ? '解除中…' : '解除する'"
          @click="confirmDetach"
        />
      </template>
    </Dialog>
  </section>
</template>

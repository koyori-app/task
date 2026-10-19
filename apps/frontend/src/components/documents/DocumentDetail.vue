<script setup lang="ts">
import { ListPlus, Loader2, Pencil, RefreshCw } from '@lucide/vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { refDebounced } from '@vueuse/core';
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue';

import MarkdownEditor from '@/components/markdown/MarkdownEditor.vue';
import CreateTaskDialog from '@/components/tasks/CreateTaskDialog.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { renderOnServer, TRANSIENT_FAILURE_RETRIES } from '@/composables/useRenderedDescription';
import type { components } from '@/generated/api';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import {
  apiClient,
  DRIVE_FILE_CONTENT_UPDATE_PATH,
  DRIVE_FILE_PATH,
  DRIVE_FILES_PATH,
  driveFileContentQueryOptions,
  driveFileQueryOptions,
  driveFileTasksQueryOptions,
  fetchClient,
  projectRootFoldersQueryOptions,
} from '@/lib/api-vue-query';
import {
  documentFileName,
  documentTitle,
  isDocumentFile,
  MAX_DOCUMENT_TITLE_LENGTH,
} from '@/lib/documents/name';
import { selectionTitle } from '@/lib/documents/selection';
import { formatTaskDate } from '@/lib/task-display';
// KFM のサイドカー CSS（タスク詳細の TaskDetailHub と同じ組）
import '@/lib/remark-gfm/style.css';
import '@/lib/remark-koyori-alerts/style.css';
import '@/lib/rehype-starry-night/style.css';
import '@/lib/rehype-kfm-code/style.css';
import '@/lib/remark-kfm-mermaid/style.css';

type CreatedTask = components['schemas']['TaskDetailResponse'];

const ATTACHMENTS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/attachments' as const;
const STATUSES_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/statuses' as const;
/** 編集中のプレビューを描き直すまでの待ち時間 */
const PREVIEW_DEBOUNCE_MS = 500;
const LEAVE_CONFIRM = '保存していない変更があります。破棄して移動しますか？';

const props = defineProps<{
  tenantId: TenantUuid;
  tenantSlug: string;
  projectId: ProjectUuid;
  projectKey: string;
  fileId: string;
  /** URL の `?edit=1` */
  initialEditing: boolean;
  /** サーバ（+data.ts）が描画した本文 HTML と、その描画元。描画元が最新本文と一致するときだけ使う */
  initialHtml: string | null;
  initialSource: string | null;
}>();

const queryClient = useQueryClient();

const rootQuery = useQuery(
  computed(() => projectRootFoldersQueryOptions(props.tenantId, props.projectId)),
);
const metaQuery = useQuery(
  computed(() => ({ ...driveFileQueryOptions(props.tenantId, props.fileId), retry: false })),
);
const contentQuery = useQuery(
  computed(() => ({ ...driveFileContentQueryOptions(props.fileId), retry: false })),
);
const tasksQuery = useQuery(
  computed(() => driveFileTasksQueryOptions(props.tenantId, props.fileId)),
);

const meta = computed(() => metaQuery.data.value ?? null);
const content = computed(() => contentQuery.data.value?.body ?? null);
const title = computed(() => (meta.value ? documentTitle(meta.value.name) : ''));
const linkedTasks = computed(() => tasksQuery.data.value?.tasks ?? []);

const isLoading = computed(
  () => rootQuery.isPending.value || metaQuery.isPending.value || contentQuery.isPending.value,
);
const isError = computed(
  () => rootQuery.isError.value || metaQuery.isError.value || contentQuery.isError.value,
);
// プロジェクトのドキュメント置き場の直下に無いファイルと、.md 以外のファイルはこの画面では扱わない
// （一覧は mime_type で絞っているが URL には任意のファイル ID を入れられる。保存で名前が `.md` に変わる）
const isNotFound = computed(
  () =>
    !!meta.value &&
    rootQuery.isSuccess.value &&
    (meta.value.folder_id !== (rootQuery.data.value?.[0]?.id ?? null) ||
      !isDocumentFile(meta.value)),
);

// ---- 編集 ----
const editing = ref(props.initialEditing);
const draftTitle = ref('');
/** 下書きの元にしたタイトル。改名はここから変えたときだけ送る（取り直したメタデータと比べると、他の人の改名を戻してしまう） */
const draftBaseTitle = ref('');
const draftBody = ref('');
const draftReady = ref(false);
/** 下書きの元にした本文の ETag。`If-Match` はこれを送る（再取得で差し替えない） */
const draftBaseEtag = ref<string | null>(null);
const saving = ref(false);
const saveError = ref<string | null>(null);
const conflict = ref(false);
const reloading = ref(false);

function resetDraft() {
  const current = meta.value;
  const loaded = contentQuery.data.value;
  if (!current || !loaded) return;
  draftTitle.value = documentTitle(current.name);
  draftBaseTitle.value = draftTitle.value;
  draftBody.value = loaded.body;
  // 本文と同じ応答の ETag を使う。メタデータと本文は別々に取るので、その間に他の人が
  // 保存するとメタデータの ETag だけが新しくなり、古い本文の下書きで上書きしてしまう
  draftBaseEtag.value = loaded.etag ?? current.etag;
  draftReady.value = true;
}

watch(
  [editing, meta, content],
  () => {
    if (!editing.value || draftReady.value) return;
    resetDraft();
  },
  { immediate: true },
);

const dirty = computed(
  () =>
    editing.value &&
    draftReady.value &&
    (draftBody.value !== content.value || draftTitle.value.trim() !== draftBaseTitle.value),
);

function setEditUrl(on: boolean) {
  const url = new URL(window.location.href);
  if (on) url.searchParams.set('edit', '1');
  else url.searchParams.delete('edit');
  window.history.replaceState(window.history.state, '', url);
}

function startEditing() {
  editing.value = true;
  setEditUrl(true);
}

function stopEditing() {
  editing.value = false;
  draftReady.value = false;
  saveError.value = null;
  conflict.value = false;
  setEditUrl(false);
}

function cancel() {
  if (dirty.value && !window.confirm(LEAVE_CONFIRM)) return;
  stopEditing();
}

const metaKey = computed(() => driveFileQueryOptions(props.tenantId, props.fileId).queryKey);
const tasksKey = computed(() => driveFileTasksQueryOptions(props.tenantId, props.fileId).queryKey);

async function save() {
  const current = meta.value;
  if (saving.value || !current || !draftReady.value) return;
  saveError.value = null;
  const nextTitle = draftTitle.value.trim();
  if (!nextTitle) {
    saveError.value = 'タイトルを入力してください';
    return;
  }
  if (Array.from(nextTitle).length > MAX_DOCUMENT_TITLE_LENGTH) {
    saveError.value = `タイトルは ${MAX_DOCUMENT_TITLE_LENGTH} 文字以内で入力してください`;
    return;
  }

  saving.value = true;
  try {
    const path = { tenant_id: props.tenantId, id: props.fileId };
    let updated = current;
    if (draftBody.value !== content.value) {
      const body = draftBody.value;
      // 下書きの元にした本文の ETag を渡し、その後に他の人が更新していたら 412 で止める
      const { data, error, response } = await fetchClient.PUT(DRIVE_FILE_CONTENT_UPDATE_PATH, {
        params: { path, header: { 'If-Match': draftBaseEtag.value ?? current.etag } },
        body: { content: body },
      });
      if (response.status === 412) {
        conflict.value = true;
        return;
      }
      if (error) {
        saveError.value =
          response.status === 413 ? '保存容量の上限を超えています' : '保存できませんでした';
        return;
      }
      updated = data;
      draftBaseEtag.value = data.etag;
      queryClient.setQueryData(driveFileContentQueryOptions(props.fileId).queryKey, {
        body,
        etag: data.etag,
      });
      // 本文の KEY-N から関連タスクが変わる
      void queryClient.invalidateQueries({ queryKey: tasksKey.value });
    }
    if (nextTitle !== draftBaseTitle.value) {
      const { data, error } = await fetchClient.PATCH(DRIVE_FILE_PATH, {
        params: { path },
        body: { name: documentFileName(nextTitle) },
      });
      if (error) {
        queryClient.setQueryData(metaKey.value, updated);
        saveError.value = '本文は保存しましたが、タイトルを変更できませんでした';
        return;
      }
      updated = data;
    }
    queryClient.setQueryData(metaKey.value, updated);
    void queryClient.invalidateQueries({ queryKey: ['get', DRIVE_FILES_PATH] });
    stopEditing();
  } finally {
    saving.value = false;
  }
}

async function refetchLatest() {
  reloading.value = true;
  try {
    await Promise.all([metaQuery.refetch(), contentQuery.refetch()]);
  } finally {
    reloading.value = false;
  }
}

/** 412 の後に最新を取り直し、下書きを捨てて最新の本文から編集し直す */
async function reloadLatest() {
  await refetchLatest();
  resetDraft();
  conflict.value = false;
}

/** 412 の後に、他の人の更新を承知のうえで自分の下書きで上書きする（新しい ETag はこのときだけ使う） */
async function overwriteWithDraft() {
  await refetchLatest();
  const latest = contentQuery.data.value;
  if (!latest) return;
  draftBaseEtag.value = latest.etag ?? meta.value?.etag ?? null;
  conflict.value = false;
  await save();
}

function onKeydown(event: KeyboardEvent) {
  if (!editing.value) return;
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
    event.preventDefault();
    void save();
  }
}

function onBeforeUnload(event: BeforeUnloadEvent) {
  if (dirty.value) event.preventDefault();
}

// クライアント遷移（リンク）は beforeunload を通らないので、捕捉段階でリンクの
// クリックを止める。shortcut: ブラウザの戻る（popstate）は止めない。必要になったら
// vike の遷移フックで扱う
function onLinkClick(event: MouseEvent) {
  if (!dirty.value) return;
  const anchor = (event.target as Element | null)?.closest?.('a[href]');
  if (!anchor || window.confirm(LEAVE_CONFIRM)) return;
  event.preventDefault();
  event.stopPropagation();
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  window.addEventListener('beforeunload', onBeforeUnload);
  window.addEventListener('click', onLinkClick, true);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
  window.removeEventListener('beforeunload', onBeforeUnload);
  window.removeEventListener('click', onLinkClick, true);
});

// ---- 本文の描画 ----
const debouncedDraft = refDebounced(draftBody, PREVIEW_DEBOUNCE_MS);
/** 描画するテキスト。編集中は下書き（デバウンス後）、閲覧中は保存済みの本文 */
const renderSource = computed(() =>
  editing.value ? (draftReady.value ? debouncedDraft.value : null) : content.value,
);
const useInitialHtml = computed(
  () => !!props.initialHtml && props.initialSource === renderSource.value,
);
const renderedQuery = useQuery(
  computed(() => {
    const text = renderSource.value ?? '';
    return {
      queryKey: ['render-description', `doc-${props.fileId}`, props.tenantSlug, text] as const,
      queryFn: () =>
        renderOnServer({
          description: text,
          scope: `doc-${props.fileId}`,
          tenant: props.tenantSlug,
        }),
      enabled: !!text && !useInitialHtml.value,
      staleTime: Infinity,
      gcTime: 5 * 60 * 1000,
      retry: TRANSIENT_FAILURE_RETRIES,
    };
  }),
);
const html = computed(() => {
  if (useInitialHtml.value) return props.initialHtml;
  const rendered = renderedQuery.data.value;
  return rendered && rendered.source === renderSource.value ? rendered.html : null;
});

// ---- 選択範囲からタスクを作る ----
const bodyElement = useTemplateRef<HTMLElement>('body');
const createTaskOpen = ref(false);
const createTaskTitle = ref('');
const linkError = ref<string | null>(null);

// ステータスはタスク作成ダイアログでしか使わないので、開くまで取らない
const statusesQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', STATUSES_PATH, {
      params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
    }),
    enabled: createTaskOpen.value,
  })),
);

function openCreateTask() {
  createTaskTitle.value = selectionTitle(window.getSelection(), bodyElement.value);
  linkError.value = null;
  createTaskOpen.value = true;
}

async function onTaskCreated(task: CreatedTask) {
  createTaskOpen.value = false;
  const { error, response } = await fetchClient.POST(ATTACHMENTS_PATH, {
    params: { path: { tenant_id: props.tenantId, project_id: props.projectId, id: task.id } },
    body: { drive_file_id: props.fileId },
  });
  // 409 は既に結び付いている（本文の参照から付いた等）ので失敗扱いにしない
  if (error && response.status !== 409) {
    linkError.value = 'タスクは作成しましたが、このドキュメントに結び付けられませんでした';
  }
  await queryClient.invalidateQueries({ queryKey: tasksKey.value });
}

function taskHref(task: { seq_key: string; project: { key: string } }) {
  return `/${props.tenantSlug}/projects/${task.project.key}/tasks/${task.seq_key}`;
}
</script>

<template>
  <div class="flex flex-col gap-4 px-4 pt-2 pb-10">
    <div v-if="isLoading" class="flex justify-center py-16">
      <Loader2 class="size-6 animate-spin text-muted-foreground" aria-label="読み込み中" />
    </div>
    <p
      v-else-if="isError || isNotFound"
      role="alert"
      class="py-16 text-center text-sm text-muted-foreground"
    >
      ドキュメントが見つかりません
    </p>

    <!-- 編集 -->
    <form
      v-else-if="editing && draftReady"
      class="flex flex-col gap-3"
      aria-label="ドキュメントを編集"
      @submit.prevent="save"
    >
      <div class="space-y-1.5">
        <Label for="document-edit-title">タイトル</Label>
        <Input
          id="document-edit-title"
          v-model="draftTitle"
          name="title"
          autocomplete="off"
          :maxlength="MAX_DOCUMENT_TITLE_LENGTH"
          class="text-lg font-semibold"
        />
      </div>

      <div
        v-if="conflict"
        role="alert"
        class="flex flex-wrap items-center gap-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive"
      >
        他の人が更新しました。最新を読み込んで下書きを捨てるか、自分の下書きで上書きしてください
        <span class="ml-auto flex gap-2">
          <Button
            type="button"
            size="sm"
            variant="outline"
            :disabled="reloading"
            @click="reloadLatest"
          >
            <RefreshCw class="size-4" :class="{ 'animate-spin': reloading }" />
            最新を読み込む
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            :disabled="reloading"
            @click="overwriteWithDraft"
          >
            自分の下書きで上書き
          </Button>
        </span>
      </div>
      <p v-if="saveError" role="alert" class="text-sm text-destructive">{{ saveError }}</p>

      <div class="grid gap-4 lg:grid-cols-2">
        <MarkdownEditor
          v-model="draftBody"
          aria-label="本文"
          placeholder="Markdown で書けます"
          min-height-class="min-h-96"
        />
        <section aria-label="プレビュー" class="min-w-0 rounded-md border p-4">
          <div v-if="html" class="kfm-content" v-html="html" />
          <p v-else class="text-sm whitespace-pre-wrap text-muted-foreground">
            {{ debouncedDraft || 'プレビューはここに表示されます' }}
          </p>
        </section>
      </div>

      <div class="flex items-center gap-2">
        <Button type="submit" :disabled="saving || conflict">
          <Loader2 v-if="saving" class="mr-2 size-4 animate-spin" />
          保存
        </Button>
        <Button type="button" variant="outline" :disabled="saving" @click="cancel">
          キャンセル
        </Button>
        <span class="text-xs text-muted-foreground">⌘S / Ctrl+S で保存</span>
      </div>
    </form>

    <!-- 閲覧 -->
    <div v-else-if="meta" class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_16rem]">
      <article class="min-w-0">
        <header class="mb-4 flex flex-wrap items-start gap-2">
          <div class="min-w-0 flex-1">
            <h1 class="text-2xl font-semibold break-words">{{ title }}</h1>
            <p class="text-xs text-muted-foreground">
              更新 <time :datetime="meta.updated_at">{{ formatTaskDate(meta.updated_at) }}</time>
            </p>
          </div>
          <Button size="sm" variant="outline" @mousedown.prevent @click="openCreateTask">
            <ListPlus class="size-4" />
            タスクを作成
          </Button>
          <Button size="sm" @click="startEditing">
            <Pencil class="size-4" />
            編集
          </Button>
        </header>
        <p v-if="linkError" role="alert" class="mb-2 text-sm text-destructive">{{ linkError }}</p>
        <div ref="body">
          <div v-if="html" class="kfm-content" v-html="html" />
          <p v-else-if="content" class="text-sm whitespace-pre-wrap">{{ content }}</p>
          <p v-else class="text-sm text-muted-foreground">本文はまだありません</p>
        </div>
      </article>

      <aside aria-labelledby="document-linked-tasks" class="flex flex-col gap-2">
        <h2 id="document-linked-tasks" class="text-sm font-semibold">関連タスク</h2>
        <p v-if="tasksQuery.isPending.value" class="text-xs text-muted-foreground">読み込み中…</p>
        <p v-else-if="tasksQuery.isError.value" class="text-xs text-destructive">
          関連タスクを読み込めませんでした
        </p>
        <p v-else-if="linkedTasks.length === 0" class="text-xs text-muted-foreground">
          本文に KEY-N と書くか「タスクを作成」で結び付きます
        </p>
        <ul v-else class="flex flex-col gap-1">
          <li v-for="task in linkedTasks" :key="task.task_id">
            <a
              :href="taskHref(task)"
              class="flex flex-col gap-0.5 rounded-md px-2 py-1.5 text-sm hover:bg-muted/50"
            >
              <span class="flex items-center gap-1.5">
                <span class="font-mono text-xs text-muted-foreground">{{ task.seq_key }}</span>
                <span
                  v-if="task.source === 'manual'"
                  class="rounded bg-muted px-1 text-[10px] text-muted-foreground"
                  title="手動で結び付けたタスク"
                  data-testid="manual-link"
                >
                  手動
                </span>
              </span>
              <span class="truncate">{{ task.title }}</span>
              <span class="flex items-center gap-1 text-xs text-muted-foreground">
                <span
                  class="inline-block size-2 rounded-full"
                  :style="{ backgroundColor: task.status.color }"
                  aria-hidden="true"
                />
                {{ task.status.name }}
              </span>
            </a>
          </li>
        </ul>
      </aside>
    </div>

    <CreateTaskDialog
      v-model:open="createTaskOpen"
      :tenant-id="tenantId"
      :project-id="projectId"
      :project-key="projectKey"
      :statuses="statusesQuery.data.value ?? []"
      :initial-title="createTaskTitle"
      @created="onTaskCreated"
    />
  </div>
</template>

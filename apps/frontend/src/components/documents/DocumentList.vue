<script setup lang="ts">
import { FileText, Loader2, Plus, Search } from '@lucide/vue';
import { useQuery } from '@tanstack/vue-query';
import { refDebounced } from '@vueuse/core';
import { computed, ref, watch } from 'vue';
import { navigate } from 'vike/client/router';

import CreateDocumentDialog from '@/components/documents/CreateDocumentDialog.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { components } from '@/generated/api';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import {
  DOCUMENT_LIST_LIMIT,
  projectDocumentsQueryOptions,
  projectRootFoldersQueryOptions,
} from '@/lib/api-vue-query';
import { documentTitle } from '@/lib/documents/name';
import { formatTaskDate } from '@/lib/task-display';

type DriveFile = components['schemas']['DriveFileResponse'];

/** 入力から一覧の再取得と URL の書き換えまでの待ち時間 */
const SEARCH_DEBOUNCE_MS = 300;

const props = defineProps<{
  tenantId: TenantUuid;
  tenantSlug: string;
  projectId: ProjectUuid;
  projectKey: string;
  /** URL の `?q=` */
  initialQuery: string;
}>();

const query = ref(props.initialQuery);
const debouncedQuery = refDebounced(
  computed(() => query.value.trim()),
  SEARCH_DEBOUNCE_MS,
);
const createOpen = ref(false);

const rootQuery = useQuery(
  computed(() => projectRootFoldersQueryOptions(props.tenantId, props.projectId)),
);
const rootFolderId = computed(() => rootQuery.data.value?.[0]?.id ?? null);

const filesQuery = useQuery(
  computed(() => ({
    ...projectDocumentsQueryOptions(props.tenantId, rootFolderId.value, debouncedQuery.value),
    enabled: !!rootFolderId.value,
  })),
);
const files = computed(() => filesQuery.data.value?.files ?? []);
const total = computed(() => filesQuery.data.value?.total ?? 0);

const isLoading = computed(
  () => rootQuery.isPending.value || (!!rootFolderId.value && filesQuery.isPending.value),
);
const isError = computed(() => rootQuery.isError.value || filesQuery.isError.value);
const hasNoRoot = computed(() => rootQuery.isSuccess.value && !rootFolderId.value);

// 検索語は URL に写す（再読み込み・共有で同じ一覧に戻れるように）。履歴は積まない
watch(debouncedQuery, (q) => {
  const url = new URL(window.location.href);
  if (q) url.searchParams.set('q', q);
  else url.searchParams.delete('q');
  window.history.replaceState(window.history.state, '', url);
});

function documentHref(file: DriveFile) {
  return `/${props.tenantSlug}/projects/${props.projectKey}/docs/${file.id}`;
}

function onCreated(file: DriveFile) {
  createOpen.value = false;
  void navigate(`${documentHref(file)}?edit=1`);
}
</script>

<template>
  <div class="mx-auto flex w-full max-w-4xl flex-col gap-4 px-4 pt-2 pb-10">
    <div class="flex items-center gap-2">
      <h1 class="text-xl font-semibold">ドキュメント</h1>
      <Button class="ml-auto" size="sm" :disabled="!rootFolderId" @click="createOpen = true">
        <Plus class="size-4" />
        新規作成
      </Button>
    </div>

    <div class="relative">
      <Search
        class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden="true"
      />
      <Input
        v-model="query"
        type="search"
        name="q"
        aria-label="ドキュメントを検索"
        placeholder="タイトルで検索"
        class="pl-8"
      />
    </div>

    <div v-if="isLoading" class="flex justify-center py-16">
      <Loader2 class="size-6 animate-spin text-muted-foreground" aria-label="読み込み中" />
    </div>
    <p v-else-if="isError" role="alert" class="py-16 text-center text-sm text-destructive">
      ドキュメントを読み込めませんでした
    </p>
    <p v-else-if="hasNoRoot" class="py-16 text-center text-sm text-muted-foreground">
      ドキュメント置き場がありません
    </p>
    <p v-else-if="files.length === 0" class="py-16 text-center text-sm text-muted-foreground">
      {{ debouncedQuery ? '一致するドキュメントがありません' : 'ドキュメントはまだありません' }}
    </p>
    <template v-else>
      <ul class="divide-y rounded-lg border">
        <li v-for="file in files" :key="file.id">
          <a
            :href="documentHref(file)"
            class="flex items-center gap-3 px-4 py-3 text-sm hover:bg-muted/50"
          >
            <FileText class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            <span class="min-w-0 flex-1 truncate font-medium">{{ documentTitle(file.name) }}</span>
            <time :datetime="file.updated_at" class="shrink-0 text-xs text-muted-foreground">
              {{ formatTaskDate(file.updated_at) }}
            </time>
          </a>
        </li>
      </ul>
      <p v-if="total > files.length" class="text-xs text-muted-foreground">
        先頭の {{ DOCUMENT_LIST_LIMIT }} 件を表示しています（全
        {{ total }} 件）。検索で絞り込んでください
      </p>
    </template>

    <CreateDocumentDialog
      v-if="rootFolderId"
      v-model:open="createOpen"
      :tenant-id="tenantId"
      :folder-id="rootFolderId"
      @created="onCreated"
    />
  </div>
</template>

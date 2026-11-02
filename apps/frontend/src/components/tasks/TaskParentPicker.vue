<script setup lang="ts">
import { X } from '@lucide/vue';
import { useQuery } from '@tanstack/vue-query';
import { refDebounced } from '@vueuse/core';
import { computed, nextTick, onMounted, ref } from 'vue';

import { Button, Input } from '@koyori-app/ui-vue';
import { GET_TASK_PATH } from '@/composables/useTaskDetail';
import { fetchClient, taskSearchQueryOptions } from '@/lib/api-vue-query';
import { taskSeqKey } from '@/lib/task-display';

const SEARCH_DEBOUNCE_MS = 300;
const SEARCH_LIMIT = 20;

const props = defineProps<{
  tenantId: string | null | undefined;
  projectId: string | null | undefined;
  projectKey: string;
  /** 候補から除くタスク。深い子孫までは手元に無いので、残りは backend の 409 に任せる */
  excludeTaskIds: string[];
  disabled: boolean;
}>();

const emit = defineEmits<{
  select: [taskId: string];
  cancel: [];
}>();

const query = ref('');
const trimmedQuery = computed(() => query.value.trim());
const debouncedQuery = refDebounced(trimmedQuery, SEARCH_DEBOUNCE_MS);
const canFetch = computed(() => !!props.tenantId && !!props.projectId && !!debouncedQuery.value);

const searchQuery = useQuery(
  computed(() => ({
    ...taskSearchQueryOptions(props.tenantId ?? '', props.projectId ?? '', debouncedQuery.value, {
      limit: SEARCH_LIMIT,
    }),
    enabled: canFetch.value,
  })),
);

// 検索 API はタイトルと説明だけを見てキーに当たらないので、`ENG-42` と `42` は 1 件を直接引く
const typedSeqKey = computed(() => {
  const upper = debouncedQuery.value.toUpperCase();
  const prefix = `${props.projectKey.toUpperCase()}-`;
  const digits = upper.startsWith(prefix) ? upper.slice(prefix.length) : upper;
  return /^\d+$/.test(digits) ? taskSeqKey(props.projectKey, Number(digits)) : null;
});

// キーは useTaskDetail と同じ形（404 は null）。同じタスクのキャッシュを共有する
const keyQuery = useQuery(
  computed(() => {
    const path = {
      tenant_id: props.tenantId ?? '',
      project_id: props.projectId ?? '',
      id: typedSeqKey.value ?? '',
    };
    return {
      queryKey: ['get', GET_TASK_PATH, { params: { path } }],
      queryFn: async ({ signal }: { signal: AbortSignal }) => {
        const { data, error, response } = await fetchClient.GET(GET_TASK_PATH, {
          params: { path },
          signal,
        });
        if (response.status === 404) return null;
        if (error) throw error;
        return data;
      },
      enabled: canFetch.value && !!typedSeqKey.value,
      retry: false,
    };
  }),
);

// debounce 中は入力と問い合わせがずれるので、追いつくまで前の語の候補を出さない
const caughtUp = computed(() => debouncedQuery.value === trimmedQuery.value);

const candidates = computed(() => {
  if (!caughtUp.value) return [];
  const seen = new Set(props.excludeTaskIds);
  return [keyQuery.data.value, ...(searchQuery.data.value?.tasks ?? [])].flatMap((task) => {
    if (!task || seen.has(task.id)) return [];
    seen.add(task.id);
    return [task];
  });
});

const loading = computed(
  () =>
    !!trimmedQuery.value &&
    (!caughtUp.value || searchQuery.isLoading.value || keyQuery.isLoading.value),
);

const inputRef = ref<InstanceType<typeof Input> | null>(null);

// koyori の Input の root は枠の span なので、中の input に当てる
onMounted(async () => {
  await nextTick();
  (inputRef.value?.$el as HTMLElement | undefined)?.querySelector('input')?.focus();
});
</script>

<template>
  <div class="mb-4 flex flex-col gap-1.5" data-task-parent-picker>
    <div class="flex min-w-0 items-center gap-1.5">
      <Input
        ref="inputRef"
        class="min-w-0 flex-1"
        aria-label="親にするタスクを検索"
        placeholder="キーまたはタイトルで検索"
        autocomplete="off"
        :value="query"
        :on-value-change="(v: string) => (query = v)"
        @keydown.esc.prevent="emit('cancel')"
      />
      <Button
        type="button"
        variant="ghost"
        class="shrink-0"
        ariaLabel="親タスクの選択をやめる"
        @click="emit('cancel')"
      >
        <template #icon><X class="size-4" aria-hidden="true" /></template>
      </Button>
    </div>

    <ul v-if="candidates.length" class="border-y border-border/60">
      <li
        v-for="task in candidates"
        :key="task.id"
        class="border-b border-border/60 last:border-b-0"
      >
        <button
          type="button"
          class="flex w-full min-w-0 items-center gap-2 px-1 py-2 text-left transition-colors hover:bg-muted/40 disabled:opacity-50"
          :disabled="disabled"
          @click="emit('select', task.id)"
        >
          <span class="shrink-0 font-mono text-xs text-muted-foreground">
            {{ taskSeqKey(projectKey, task.seq_id) }}
          </span>
          <span class="truncate text-sm">{{ task.title }}</span>
        </button>
      </li>
    </ul>
    <p v-else-if="loading" class="py-1 text-sm text-muted-foreground">検索中…</p>
    <p
      v-else-if="searchQuery.isError.value || keyQuery.isError.value"
      class="py-1 text-sm text-destructive"
      role="alert"
    >
      検索できませんでした
    </p>
    <p v-else-if="trimmedQuery" class="py-1 text-sm text-muted-foreground">
      一致する候補がありません
    </p>
  </div>
</template>

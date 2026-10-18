<script setup lang="ts">
import { computed, ref } from 'vue';
import { PhCheck } from '@phosphor-icons/vue';
import {
  ListboxContent,
  ListboxFilter,
  ListboxItem,
  ListboxItemIndicator,
  ListboxRoot,
} from 'reka-ui';

import UserAvatar from '@/components/UserAvatar.vue';
import { Button } from '@/components/ui/button';
import type { SelectableUser, UserSelectorState } from '@/components/users/types';
import { rankUsersByQuery } from '@/lib/user-search';

/**
 * 利用者の選択の中身（絞り込みの窓・状態・候補の listbox）。Popover の中で使う
 * （`UserSelector`）。単体で描けるので、試験はこちらを直に組む。
 *
 * - キーボード: 上下で候補を移り、Enter で選ぶ（複数なら付け外し）。打った字で絞り込む
 * - IME: 変換中の字では絞り込まず、変換中の Enter では選ばない（reka の ListboxFilter と
 *   ListboxRoot が composition を見ている）
 * - 並びは渡された順のまま。絞り込んだ時だけ前方一致・部分一致の順にする
 * - 描く項は先頭の `maxRendered` 人まで（既定 100）。超えた分は数だけ添え、名での絞り込みを
 *   促す。数百人を一度に描くと、打つ字ごとの描き直しが重くなるため（仮想化の依存は足さぬ）
 */
const props = withDefaults(
  defineProps<{
    users: SelectableUser[];
    /** 選んだ利用者の id。単数でも配列で受ける（0 か 1 件） */
    modelValue: string[];
    multiple?: boolean;
    disabled?: boolean;
    state?: UserSelectorState;
    /** 自分の id。候補に「自分」と添える */
    currentUserId?: string | null;
    /** 絞り込みの窓の名（aria-label と placeholder） */
    searchLabel?: string;
    /** 取得できて候補が 0 人の時の文 */
    emptyText?: string;
    noMatchText?: string;
    autoFocus?: boolean;
    /** 一度に描く項の上限 */
    maxRendered?: number;
  }>(),
  {
    multiple: true,
    searchLabel: '利用者を検索',
    emptyText: '選べる利用者がいません',
    noMatchText: '一致する利用者がいません',
    autoFocus: false,
    maxRendered: 100,
  },
);

const emit = defineEmits<{
  'update:modelValue': [userIds: string[]];
  /** 一人の付け外し。担当者のように一人ずつ保存する呼び手のため */
  toggle: [userId: string, checked: boolean];
}>();

const query = ref('');
const visible = computed(() => rankUsersByQuery(props.users, query.value));
const rendered = computed(() => visible.value.slice(0, props.maxRendered));
const hiddenCount = computed(() => visible.value.length - rendered.value.length);
const showList = computed(
  () => !props.state?.loading && !props.state?.error && visible.value.length > 0,
);

const listboxValue = computed(() =>
  props.multiple ? props.modelValue : (props.modelValue[0] ?? undefined),
);

function onUpdate(value: unknown) {
  const next = (Array.isArray(value) ? value : value == null ? [] : [value]).map(
    String,
  ) as string[];
  const before = new Set(props.modelValue);
  const after = new Set(next);
  for (const id of next) if (!before.has(id)) emit('toggle', id, true);
  for (const id of props.modelValue) if (!after.has(id)) emit('toggle', id, false);
  emit('update:modelValue', next);
}
</script>

<template>
  <ListboxRoot
    :model-value="listboxValue"
    :multiple="multiple"
    :selection-behavior="multiple ? 'toggle' : 'replace'"
    :disabled="disabled"
    highlight-on-hover
    class="flex flex-col"
    data-slot="user-selector"
    @update:model-value="onUpdate"
  >
    <div class="border-b p-1">
      <ListboxFilter
        v-model="query"
        :auto-focus="autoFocus"
        :aria-label="searchLabel"
        :placeholder="searchLabel"
        class="h-8 w-full rounded-sm bg-transparent px-2 text-sm outline-hidden placeholder:text-muted-foreground"
      />
    </div>

    <p
      v-if="state?.loading"
      role="status"
      aria-busy="true"
      class="px-2 py-1.5 text-sm text-muted-foreground"
    >
      読み込み中…
    </p>
    <div v-else-if="state?.error" role="alert" class="flex items-center gap-2 px-2 py-1.5">
      <p class="text-sm text-destructive">候補を読み込めませんでした</p>
      <Button
        v-if="state.onRetry"
        type="button"
        variant="outline"
        size="sm"
        class="h-7 text-xs"
        @click="state.onRetry()"
      >
        再試行
      </Button>
    </div>
    <p v-else-if="!users.length" role="status" class="px-2 py-1.5 text-sm text-muted-foreground">
      {{ emptyText }}
    </p>
    <p v-else-if="!visible.length" role="status" class="px-2 py-1.5 text-sm text-muted-foreground">
      {{ noMatchText }}
    </p>

    <ListboxContent
      v-show="showList"
      :aria-label="searchLabel"
      class="max-h-72 overflow-y-auto p-1"
    >
      <ListboxItem
        v-for="user in rendered"
        :key="user.id"
        :value="user.id"
        :data-user-id="user.id"
        class="relative flex cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground"
      >
        <span class="pointer-events-none absolute left-2 flex size-3.5 items-center justify-center">
          <ListboxItemIndicator>
            <PhCheck class="size-4" />
          </ListboxItemIndicator>
        </span>
        <!-- 頭文字の字が項の名に混じらぬよう、アバターは支援技術から隠す -->
        <span aria-hidden="true" class="flex shrink-0">
          <UserAvatar :user="user" size="sm" decorative />
        </span>
        <span class="min-w-0 truncate">{{ user.username }}</span>
        <span
          v-if="user.id === currentUserId"
          class="ml-auto shrink-0 text-xs text-muted-foreground"
        >
          自分
        </span>
      </ListboxItem>
    </ListboxContent>
    <p
      v-if="showList && hiddenCount"
      role="status"
      class="border-t px-2 py-1.5 text-xs text-muted-foreground"
    >
      ほか {{ hiddenCount }} 人。名で絞り込めます
    </p>
  </ListboxRoot>
</template>

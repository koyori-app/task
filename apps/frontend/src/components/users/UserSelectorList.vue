<script setup lang="ts">
import { computed, ref } from 'vue';
import { ListboxContent, ListboxFilter, ListboxRoot, ListboxVirtualizer } from 'reka-ui';

import { Button } from '@koyori-app/ui-vue';
import type { SelectableUser, UserSelectorState } from '@/components/users/types';
import UserSelectorOption from '@/components/users/UserSelectorOption.vue';
import { rankUsersByQuery } from '@/lib/user-search';

/**
 * 利用者の選択の中身（絞り込みの窓・状態・候補の listbox）。Popover の中で使う
 * （`UserSelector`）。単体で描けるので、試験はこちらを直に組む。
 *
 * - キーボード: 上下で候補を移り、Enter で選ぶ（複数なら付け外し）。打った字で絞り込む
 * - IME: 変換中の字では絞り込まず、変換中の Enter では選ばない（reka の ListboxFilter と
 *   ListboxRoot が composition を見ている）
 * - 並びは渡された順のまま。絞り込んだ時だけ前方一致・部分一致の順にする
 * - 候補は仮想化して全員を並べる（reka の ListboxVirtualizer）。DOM に載るのは見えている項と
 *   前後の数項だけで、数千人でも打つ字ごとの描き直しは重くならない。上下・Home・End で見えぬ項へ
 *   移ると一覧が送られ、照らした項は ListboxFilter の aria-activedescendant で伝わる
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
  }>(),
  {
    multiple: true,
    searchLabel: '利用者を検索',
    emptyText: '選べる利用者がいません',
    noMatchText: '一致する利用者がいません',
    autoFocus: false,
  },
);

const emit = defineEmits<{
  'update:modelValue': [userIds: string[]];
  /** 一人の付け外し。担当者のように一人ずつ保存する呼び手のため */
  toggle: [userId: string, checked: boolean];
}>();

const query = ref('');
const visible = computed(() => rankUsersByQuery(props.users, query.value));
// 仮想化は options と項の value を突き合わせる（選んだ者へ送る時）。value に揃えて id を渡す
const visibleIds = computed(() => visible.value.map((u) => u.id));
const userById = computed(() => new Map(props.users.map((u) => [u.id, u])));
const userOf = (id: string) => userById.value.get(id)!;
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
        variant="tertiary"
        label="再試行"
        @click="state.onRetry()"
      />
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
      <!-- reka は項の丈を測らず見積もりで並べる。項の丈は h-8（32px）に固め、見積もりと揃える -->
      <ListboxVirtualizer
        v-slot="{ option: id }"
        :options="visibleIds"
        :estimate-size="32"
        :text-content="(id: string) => userOf(id).username.toLowerCase()"
      >
        <UserSelectorOption :user="userOf(id)" :current-user-id="currentUserId" />
      </ListboxVirtualizer>
    </ListboxContent>
  </ListboxRoot>
</template>

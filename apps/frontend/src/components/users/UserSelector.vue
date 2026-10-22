<script setup lang="ts">
import { computed, ref, useTemplateRef, watch } from 'vue';
import { useFocusWithin } from '@vueuse/core';
import { Picker } from '@koyori-app/ui-vue';

import type { SelectableUser, UserSelectorState } from '@/components/users/types';
import { orderUsersForPicker } from '@/lib/user-search';

/**
 * 利用者を選ぶ部品。利用者を選ぶ所は全てこれを通す（中身は koyori の Picker）。
 *
 * - 候補は自分を先頭に、その他は名の順。自分の名には「（自分）」を添える。絞り込みは名の部分一致
 * - 単数（`multiple=false`）では選ぶと閉じ、複数では開いたまま続けて選べる
 * - トリガーの中身を替える呼び手は `#trigger` で描く（Picker のボタンの内側に入る）
 * - 保存中（`disabled`）は選択を受けない。Picker の disabled は開いた一覧を隠すので、フォーカスが
 *   中にある間（開いている間）は渡さない。一人ずつ保存する担当者で、選ぶたびに一覧が閉じてしまう
 */
const props = withDefaults(
  defineProps<{
    /** Picker の名。Field の中なら Field の見出しと同じ文言 */
    label: string;
    users: SelectableUser[];
    modelValue: string[];
    multiple?: boolean;
    disabled?: boolean;
    state?: UserSelectorState;
    currentUserId?: string | null;
    searchLabel?: string;
    emptyText?: string;
    noMatchText?: string;
  }>(),
  { multiple: true },
);

const emit = defineEmits<{
  'update:modelValue': [userIds: string[]];
  toggle: [userId: string, checked: boolean];
}>();

const items = computed(() =>
  orderUsersForPicker(props.users, [], props.currentUserId).map((user) => ({
    value: user.id,
    label: user.id === props.currentUserId ? `${user.username}（自分）` : user.username,
    src: user.avatar_url || undefined,
  })),
);
const userIds = computed(() => new Set(props.users.map((user) => user.id)));
const { focused } = useFocusWithin(useTemplateRef('picker'));

// 選んだ値は手元で先に反映する（保存してから再取得する呼び手でも、押した項がすぐ選ばれて見える）。
// 呼び手の値に揃えるのは保存中でない時だけ。保存中に届く値は途中の物でありうる
const value = ref([...props.modelValue]);
watch([() => props.modelValue, () => props.disabled], ([ids, disabled]) => {
  if (!disabled) value.value = [...ids];
});

function onSelectionChange(ids: string[]) {
  if (props.disabled) return;
  const before = value.value;
  // Picker は候補に無い id（プロジェクトを外れた担当者など）を落として返す。複数では外さずに残す
  const next = props.multiple ? [...before.filter((id) => !userIds.value.has(id)), ...ids] : ids;
  value.value = next;
  for (const id of next) if (!before.includes(id)) emit('toggle', id, true);
  for (const id of before) if (!next.includes(id)) emit('toggle', id, false);
  emit('update:modelValue', next);
}
</script>

<template>
  <Picker
    ref="picker"
    :label="label"
    :items="items"
    avatars
    :selection-mode="multiple ? 'multiple' : 'single'"
    :selected-values="value"
    :on-selection-change="onSelectionChange"
    :loading="state?.loading"
    :error="state?.error ? '候補を読み込めませんでした' : undefined"
    :on-retry="state?.onRetry"
    :disabled="disabled && !focused"
    :search-label="searchLabel"
    :search-placeholder="searchLabel"
    :empty-message="emptyText"
    :no-results-message="noMatchText"
  >
    <template v-if="$slots.trigger" #trigger><slot name="trigger" /></template>
  </Picker>
</template>

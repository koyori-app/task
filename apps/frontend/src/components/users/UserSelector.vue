<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui';

import UserSelectorList from '@/components/users/UserSelectorList.vue';
import type { SelectableUser, UserSelectorState } from '@/components/users/types';
import { cn } from '@/lib/utils';
import { orderUsersForPicker } from '@/lib/user-search';

/**
 * 利用者を選ぶ部品。利用者を選ぶ所は全てこれを通す。
 *
 * - trigger は呼び手が `#trigger` で描く（アバターの重ね・名の並び・ボタン等）。
 *   受け取る `selectedUsers` は候補から引いた選択中の利用者
 * - 候補は開いた時に一度だけ並べる（選んだ者、自分、その他の名の順）。開いている間に
 *   付け外ししても並びは動かさない（押した項が指の下から逃げぬように）
 * - 単数（`multiple=false`）では選ぶと閉じる
 * - Esc で閉じて trigger へ focus が戻る。外を押すか Tab で外れても閉じる
 */
const props = withDefaults(
  defineProps<{
    users: SelectableUser[];
    modelValue: string[];
    multiple?: boolean;
    disabled?: boolean;
    state?: UserSelectorState;
    currentUserId?: string | null;
    searchLabel?: string;
    emptyText?: string;
    noMatchText?: string;
    align?: 'start' | 'center' | 'end';
    contentClass?: string;
  }>(),
  { multiple: true, align: 'start' },
);

const emit = defineEmits<{
  'update:modelValue': [userIds: string[]];
  toggle: [userId: string, checked: boolean];
}>();

const open = ref(false);

// 並べに使う「選んだ者」は、開いた時に凍らせる。開いている間に付け外ししても並びは
// 動かさず、候補が届いた時（取得の後）だけ並べ直す。閉じている間は今の選択に従う。
const orderSelection = ref<string[]>([...props.modelValue]);

// 一覧へ渡す選択。選んだ時に手元で先に反映し、呼び手の保存が遅れて届いても中身が同じなら
// 差し替えない。reka の ListboxRoot は、押した直後（1ms）を過ぎてから modelValue が変わると
// 選んだ者へ一覧を送り直す。担当者のように保存してから再取得する呼び手では、選ぶたびに一覧が
// 先頭の選んだ者へ巻き戻っておった。
//
// 呼び手の選択と突き合わせるのは、保存中（disabled）でない時だけにする。保存中に届く値は途中の
// 物でありうる。保存が終わって disabled が下りた時に突き合わせ、中身が違えば（保存の失敗で戻った・
// 他で変わった）呼び手に従う
const listValue = ref<string[]>([...props.modelValue]);
const sameIds = (a: string[], b: string[]) =>
  a.length === b.length && a.every((id) => b.includes(id));
function followCaller() {
  if (!sameIds(props.modelValue, listValue.value)) listValue.value = [...props.modelValue];
}

watch(open, (isOpen) => {
  if (isOpen) {
    orderSelection.value = [...props.modelValue];
    listValue.value = [...props.modelValue];
  }
});
watch(
  () => props.modelValue,
  (ids) => {
    if (!open.value) orderSelection.value = [...ids];
    if (!props.disabled) followCaller();
  },
);
watch(
  () => props.disabled,
  (disabled) => {
    if (!disabled) followCaller();
  },
);
const ordered = computed(() =>
  orderUsersForPicker(props.users, orderSelection.value, props.currentUserId),
);

const selectedUsers = computed(() => {
  const byId = new Map(props.users.map((user) => [user.id, user]));
  return props.modelValue.flatMap((id) => byId.get(id) ?? []);
});

function onUpdate(ids: string[]) {
  listValue.value = ids;
  emit('update:modelValue', ids);
  if (!props.multiple && ids.length) open.value = false;
}
</script>

<template>
  <PopoverRoot v-model:open="open">
    <PopoverTrigger as-child :disabled="disabled">
      <slot name="trigger" :selected-users="selectedUsers" :open="open" />
    </PopoverTrigger>
    <PopoverPortal>
      <PopoverContent
        :align="align"
        :side-offset="4"
        data-slot="user-selector-content"
        :class="
          cn(
            'bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2 z-50 w-64 max-w-[calc(100vw-2rem)] origin-(--reka-popover-content-transform-origin) overflow-hidden rounded-md border shadow-md outline-hidden',
            contentClass,
          )
        "
      >
        <UserSelectorList
          :users="ordered"
          :model-value="listValue"
          :multiple="multiple"
          :disabled="disabled"
          :state="state"
          :current-user-id="currentUserId"
          :search-label="searchLabel"
          :empty-text="emptyText"
          :no-match-text="noMatchText"
          auto-focus
          @toggle="(id, checked) => emit('toggle', id, checked)"
          @update:model-value="onUpdate"
        />
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>

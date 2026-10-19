<script setup lang="ts">
import { PhCheck } from '@phosphor-icons/vue';
import { ListboxItem, ListboxItemIndicator } from 'reka-ui';

import UserAvatar from '@/components/UserAvatar.vue';
import type { SelectableUser } from '@/components/users/types';

/**
 * 候補の一項（`UserSelectorList` の仮想化した一覧の中で使う）。
 *
 * 仮想化は項を位置（index）で鍵付けし、絞り込むと同じ位置に別の者が来る。reka の ListboxItem は
 * 描いた中身を照らし・選びの状態だけで溜めるので、渡した属性（`data-user-id` など）がその時に
 * 描き直されない。項を利用者の id で鍵付けし、別の者になれば作り直す。仮想化が足す位置・丈の
 * 属性はそのまま ListboxItem へ渡す。
 */
defineOptions({ inheritAttrs: false });

defineProps<{
  user: SelectableUser;
  currentUserId?: string | null;
}>();
</script>

<template>
  <ListboxItem
    :key="user.id"
    v-bind="$attrs"
    :value="user.id"
    :data-user-id="user.id"
    class="relative flex h-8 w-full cursor-default items-center gap-2 rounded-sm py-1.5 pr-2 pl-8 text-sm outline-hidden select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground"
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
    <span v-if="user.id === currentUserId" class="ml-auto shrink-0 text-xs text-muted-foreground">
      自分
    </span>
  </ListboxItem>
</template>

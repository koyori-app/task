<script setup lang="ts">
import { computed } from 'vue';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import type { components } from '@/generated/api';

type UserSummary = components['schemas']['UserSummary'];

const props = withDefaults(
  defineProps<{
    /** 担当ユーザー一覧（表示順は API 応答順） */
    users: UserSummary[];
    /** 重ね表示する最大数（超過分は +N チップ） */
    maxDisplay?: number;
  }>(),
  {
    maxDisplay: 3,
  },
);

const visibleUsers = computed(() => props.users.slice(0, props.maxDisplay));
const remaining = computed(() => Math.max(0, props.users.length - props.maxDisplay));
const firstUser = computed(() => props.users[0]);

function initials(username: string) {
  return username.slice(0, 1).toUpperCase();
}
</script>

<template>
  <div class="flex items-center gap-1.5">
    <!-- 重ねアバター群 -->
    <div class="flex -space-x-2">
      <div
        v-for="user in visibleUsers"
        :key="user.id"
        class="size-7 rounded-full ring-2 ring-background"
      >
        <Avatar class="size-full">
          <AvatarImage v-if="user.avatar_url" :src="user.avatar_url" :alt="user.username" />
          <AvatarFallback class="text-[10px] bg-muted text-muted-foreground">
            {{ initials(user.username) }}
          </AvatarFallback>
        </Avatar>
      </div>
      <!-- +N オーバーフローチップ（comp-409 準拠） -->
      <div
        v-if="remaining > 0"
        class="size-7 rounded-full bg-muted text-muted-foreground text-[10px] font-medium flex items-center justify-center ring-2 ring-background"
      >
        +{{ remaining }}
      </div>
    </div>
    <!-- 先頭名 + 他N名 テキスト（殿指示により維持） -->
    <span class="text-xs truncate max-w-28 text-muted-foreground">
      <template v-if="users.length === 1">{{ firstUser?.username }}</template>
      <template v-else-if="users.length > 1 && remaining > 0"
        >{{ firstUser?.username }} 他{{ remaining }}名</template
      >
      <template v-else-if="users.length > 1">{{ firstUser?.username }}</template>
    </span>
  </div>
</template>

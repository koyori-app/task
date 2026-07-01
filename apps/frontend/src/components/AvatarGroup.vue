<script setup lang="ts">
import { computed } from 'vue';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';

const props = withDefaults(
  defineProps<{
    /** ユーザーUUID配列 */
    userIds: string[];
    /** 重ね表示する最大数（超過分は +N チップ） */
    maxDisplay?: number;
  }>(),
  {
    maxDisplay: 3,
  },
);

const visibleIds = computed(() => props.userIds.slice(0, props.maxDisplay));
const remaining = computed(() => Math.max(0, props.userIds.length - props.maxDisplay));
const firstId = computed(() => props.userIds[0] ?? '');
</script>

<template>
  <div class="flex items-center gap-1.5">
    <!-- 重ねアバター群 -->
    <div class="flex -space-x-2">
      <div
        v-for="userId in visibleIds"
        :key="userId"
        class="size-7 rounded-full ring-2 ring-background"
      >
        <Avatar class="size-full">
          <AvatarFallback class="text-[10px] bg-muted text-muted-foreground"> ? </AvatarFallback>
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
      <template v-if="userIds.length === 1">{{ firstId.slice(0, 8) }}…</template>
      <template v-else-if="userIds.length > 1"
        >{{ firstId.slice(0, 8) }}… 他{{
          remaining > 0 ? remaining : userIds.length - 1
        }}名</template
      >
    </span>
  </div>
</template>

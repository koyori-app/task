<script setup lang="ts">
import { computed, onMounted } from 'vue';

import UserAvatar from '@/components/UserAvatar.vue';
import { emojiName, preloadEmojiData } from '@/lib/emoji-data';
import { REACTION_CARD_USER_LIMIT, type ReactionUser } from '@/lib/reactions';

const props = withDefaults(
  defineProps<{
    emoji: string;
    count: number;
    users: ReactionUser[];
    currentUserId?: string | null;
  }>(),
  { currentUserId: null },
);

defineEmits<{ showAll: [] }>();

const name = computed(() => emojiName(props.emoji));
const visibleUsers = computed(() => props.users.slice(0, REACTION_CARD_USER_LIMIT));

onMounted(preloadEmojiData);
</script>

<template>
  <div
    role="group"
    :aria-label="`${name ?? emoji}を押した人`"
    class="w-60 max-w-[calc(100vw-16px)] rounded-[12px] border border-(--koyori-color-border) bg-(--koyori-color-surface) text-(--koyori-color-text) shadow-(--koyori-shadow-panel)"
  >
    <div class="flex items-center gap-2 border-b border-(--koyori-color-border) px-3 py-2">
      <span class="text-[18px] leading-none" aria-hidden="true">{{ emoji }}</span>
      <span class="min-w-0 flex-1 truncate text-sm font-medium" :title="name ?? undefined">{{
        name
      }}</span>
      <span class="shrink-0 text-xs text-(--koyori-color-text-secondary) tabular-nums">
        {{ count }} 人
      </span>
    </div>
    <ul class="px-3 py-2">
      <li v-for="user in visibleUsers" :key="user.id" class="flex h-8 items-center gap-2">
        <UserAvatar :user="user" size="md" decorative />
        <span class="min-w-0 flex-1 truncate text-sm" :title="user.name">{{ user.name }}</span>
        <span
          v-if="user.id === currentUserId"
          class="shrink-0 text-xs text-(--koyori-color-text-secondary)"
        >
          自分
        </span>
      </li>
    </ul>
    <div v-if="count > REACTION_CARD_USER_LIMIT" class="px-3 pb-2">
      <button
        type="button"
        class="rounded-sm text-xs font-medium text-link hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
        @click="$emit('showAll')"
      >
        すべて見る（{{ count }} 人）
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Button, Skeleton, XIcon } from '@koyori-app/ui-vue';
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue';

import UserAvatar from '@/components/UserAvatar.vue';
import { emojiName, preloadEmojiData } from '@/lib/emoji-data';
import type { FetchReactionUsers, ReactionSummary, ReactionUser } from '@/lib/reactions';

const props = withDefaults(
  defineProps<{
    reactions: ReactionSummary[];
    /** 開いた時に選んでおく絵文字。無ければ先頭 */
    initialEmoji?: string | null;
    fetchUsers: FetchReactionUsers;
    profileHref?: (userId: string) => string;
    currentUserId?: string | null;
  }>(),
  { initialEmoji: null, profileHref: undefined, currentUserId: null },
);

const emit = defineEmits<{ close: [] }>();

type UsersState =
  | { status: 'loading' }
  | { status: 'error' }
  | { status: 'ready'; users: ReactionUser[] };

const tabsRef = ref<HTMLElement | null>(null);
const activeEmoji = ref(props.initialEmoji ?? props.reactions[0]?.emoji ?? '');
/** 絵文字ごとの取得結果。開いている間は取り直さない */
const usersByEmoji = reactive(new Map<string, UsersState>());

const activeSummary = computed(() =>
  props.reactions.find((reaction) => reaction.emoji === activeEmoji.value),
);
const activeState = computed(() => usersByEmoji.get(activeEmoji.value));
const activeCount = computed(() => {
  const state = activeState.value;
  return state?.status === 'ready' ? state.users.length : (activeSummary.value?.count ?? 0);
});

async function fetchActive(emoji: string) {
  usersByEmoji.set(emoji, { status: 'loading' });
  try {
    usersByEmoji.set(emoji, { status: 'ready', users: await props.fetchUsers(emoji) });
  } catch {
    usersByEmoji.set(emoji, { status: 'error' });
  }
}

watch(
  activeEmoji,
  (emoji) => {
    if (emoji && !usersByEmoji.has(emoji)) void fetchActive(emoji);
  },
  { immediate: true },
);

// 開いている間に最後の 1 人が外して札が消えたら、残りの札へ移るか閉じる
watch(
  () => props.reactions,
  (reactions) => {
    if (reactions.some((reaction) => reaction.emoji === activeEmoji.value)) return;
    const first = reactions[0];
    if (first) activeEmoji.value = first.emoji;
    else emit('close');
  },
);

function focusActiveTab() {
  tabsRef.value?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
}

function onTabKeyDown(event: KeyboardEvent) {
  const index = props.reactions.findIndex((reaction) => reaction.emoji === activeEmoji.value);
  const last = props.reactions.length - 1;
  const targets: Record<string, number> = {
    ArrowDown: index === last ? 0 : index + 1,
    ArrowUp: index <= 0 ? last : index - 1,
    Home: 0,
    End: last,
  };
  const target = targets[event.key];
  const reaction = target === undefined ? undefined : props.reactions[target];
  if (!reaction) return;
  event.preventDefault();
  activeEmoji.value = reaction.emoji;
  void nextTick(focusActiveTab);
}

function onKeyDown(event: KeyboardEvent) {
  if (event.key !== 'Escape') return;
  // タスク詳細の dialog まで閉じないよう止める
  event.preventDefault();
  event.stopPropagation();
  emit('close');
}

onMounted(() => {
  preloadEmojiData();
  focusActiveTab();
});
</script>

<template>
  <div
    role="dialog"
    aria-label="リアクション"
    class="flex h-[440px] max-h-[calc(100dvh-16px)] w-[360px] max-w-[calc(100vw-16px)] flex-col overflow-hidden rounded-[16px] border border-(--koyori-color-border) bg-(--koyori-color-surface) text-(--koyori-color-text) shadow-(--koyori-shadow-panel)"
    @keydown="onKeyDown"
  >
    <div
      class="flex shrink-0 items-center justify-between border-b border-(--koyori-color-border) py-1 pr-1 pl-3"
    >
      <h2 class="text-sm font-medium">リアクション</h2>
      <Button variant="ghost" ariaLabel="閉じる" @click="emit('close')">
        <template #icon><XIcon /></template>
      </Button>
    </div>
    <div class="flex min-h-0 flex-1">
      <div
        ref="tabsRef"
        role="tablist"
        aria-orientation="vertical"
        aria-label="絵文字"
        class="flex w-[76px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-(--koyori-color-border) p-1"
        @keydown="onTabKeyDown"
      >
        <button
          v-for="reaction in reactions"
          :key="reaction.emoji"
          type="button"
          role="tab"
          class="people-tab"
          :aria-selected="reaction.emoji === activeEmoji"
          :aria-label="`${emojiName(reaction.emoji) ?? reaction.emoji} ${reaction.count} 件`"
          :tabindex="reaction.emoji === activeEmoji ? 0 : -1"
          @click="activeEmoji = reaction.emoji"
        >
          <span class="text-base leading-none" aria-hidden="true">{{ reaction.emoji }}</span>
          <span class="text-xs tabular-nums" aria-hidden="true">{{ reaction.count }}</span>
        </button>
      </div>
      <div role="tabpanel" class="flex min-w-0 flex-1 flex-col">
        <div class="flex shrink-0 items-center gap-2 px-3 py-2 text-sm">
          <span class="min-w-0 truncate font-medium">{{
            emojiName(activeEmoji) ?? activeEmoji
          }}</span>
          <span class="shrink-0 text-xs text-(--koyori-color-text-secondary) tabular-nums">
            {{ activeCount }} 人
          </span>
        </div>
        <div
          class="min-h-0 flex-1 overflow-y-auto px-3 pb-2"
          :aria-busy="activeState?.status === 'loading'"
        >
          <template v-if="!activeState || activeState.status === 'loading'">
            <div
              v-for="index in 5"
              :key="index"
              class="flex h-11 items-center gap-2"
              data-testid="people-skeleton"
            >
              <Skeleton width="32px" height="32px" radius="50%" />
              <Skeleton width="60%" height="14px" />
            </div>
          </template>
          <div v-else-if="activeState.status === 'error'" role="alert" class="pt-2 text-sm">
            <p class="mb-2 text-destructive">読み込めませんでした</p>
            <Button
              variant="secondary"
              label="もう一度読み込む"
              @click="fetchActive(activeEmoji)"
            />
          </div>
          <p
            v-else-if="activeState.users.length === 0"
            class="pt-2 text-sm text-(--koyori-color-text-secondary)"
          >
            リアクションした人はいません
          </p>
          <ul v-else>
            <li
              v-for="user in activeState.users"
              :key="user.id"
              class="flex h-11 items-center gap-2"
            >
              <UserAvatar :user="user" size="xl" decorative />
              <component
                :is="profileHref ? 'a' : 'span'"
                :href="profileHref?.(user.id)"
                :title="user.name"
                class="min-w-0 flex-1 truncate text-sm"
                :class="profileHref && 'hover:underline'"
                >{{ user.name }}</component
              >
              <span
                v-if="user.id === currentUserId"
                class="shrink-0 text-xs text-(--koyori-color-text-secondary)"
              >
                自分
              </span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.people-tab {
  display: flex;
  height: 32px;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  gap: 4px;
  border-radius: 8px;
  color: var(--koyori-color-text-secondary);
}

.people-tab[aria-selected='true'] {
  background: var(--koyori-color-selection);
  color: var(--koyori-color-text);
}

@media (hover: hover) {
  .people-tab[aria-selected='false']:hover {
    background: var(--koyori-color-surface-hover);
  }
}

.people-tab:focus-visible {
  outline: var(--koyori-focus-width) solid var(--koyori-focus-color);
  outline-offset: calc(-1 * var(--koyori-focus-width));
}
</style>

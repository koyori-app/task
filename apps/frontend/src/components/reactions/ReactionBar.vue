<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

import { emojiKey } from '@/lib/emoji-data';
import type { FetchReactionUsers, ReactionSummary } from '@/lib/reactions';

import AnchoredPanel from './AnchoredPanel.vue';
import ReactionAddButton from './ReactionAddButton.vue';
import ReactionChip from './ReactionChip.vue';
import ReactionPeopleCard from './ReactionPeopleCard.vue';
import ReactionPeoplePanel from './ReactionPeoplePanel.vue';

/** カードを出すまでの待ち（Tooltip と同じ遅延の流儀） */
const PEEK_DELAY_MS = 400;
/** 札からカードへポインタを移す間に閉じないための猶予 */
const PEEK_HIDE_DELAY_MS = 150;

const props = withDefaults(
  defineProps<{
    reactions: ReactionSummary[];
    canReact: boolean;
    /** 札が 0 件でも追加ボタンの行を出す（概要用） */
    alwaysShowAdd?: boolean;
    busy?: boolean;
    error?: string | null;
    fetchUsers: FetchReactionUsers;
    profileHref?: (userId: string) => string;
    currentUserId?: string | null;
  }>(),
  {
    alwaysShowAdd: false,
    busy: false,
    error: null,
    profileHref: undefined,
    currentUserId: null,
  },
);

const emit = defineEmits<{ toggle: [emoji: string]; add: [emoji: string] }>();

const rootRef = ref<HTMLElement | null>(null);

const showRow = computed(
  () => props.reactions.length > 0 || (props.alwaysShowAdd && props.canReact),
);
const selected = computed(() =>
  props.reactions.filter((reaction) => reaction.reacted).map((reaction) => reaction.emoji),
);

function onToggle(emoji: string) {
  if (props.canReact && !props.busy) emit('toggle', emoji);
}

/** 押している絵文字をピッカーで選び直したら外す（札を押すのと同じ） */
function onPick(emoji: string) {
  if (props.busy) return;
  const isSelected = selected.value.some((item) => emojiKey(item) === emojiKey(emoji));
  if (isSelected) emit('toggle', emoji);
  else emit('add', emoji);
}

// ─── 押した人のカード ───

const peek = ref<{ emoji: string; anchor: HTMLElement } | null>(null);
const peekReaction = computed(() =>
  props.reactions.find((reaction) => reaction.emoji === peek.value?.emoji),
);
let showTimer: ReturnType<typeof setTimeout> | undefined;
let hideTimer: ReturnType<typeof setTimeout> | undefined;

function schedulePeek(emoji: string, anchor: HTMLElement) {
  clearTimeout(hideTimer);
  clearTimeout(showTimer);
  showTimer = setTimeout(() => {
    if (!panel.value) peek.value = { emoji, anchor };
  }, PEEK_DELAY_MS);
}

function scheduleHide() {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    peek.value = null;
  }, PEEK_HIDE_DELAY_MS);
}

function cancelHide() {
  clearTimeout(hideTimer);
}

function hidePeekNow() {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  peek.value = null;
}

function onChipPointerEnter(event: PointerEvent, emoji: string) {
  // タッチの札は長押しで全員の一覧を開く
  if (event.pointerType !== 'touch') schedulePeek(emoji, event.currentTarget as HTMLElement);
}

function isFocusVisible(element: HTMLElement) {
  try {
    return element.matches(':focus-visible');
  } catch {
    return true;
  }
}

function onChipFocus(event: FocusEvent, emoji: string) {
  const chip = event.currentTarget as HTMLElement;
  // マウスで押した時のフォーカスでは出さない（ホバーの経路で出る）
  if (isFocusVisible(chip)) schedulePeek(emoji, chip);
}

function onChipBlur(event: FocusEvent) {
  // 札の次の Tab はカードの中の「すべて見る」へ入る
  if (!isInCard(event.relatedTarget)) scheduleHide();
}

function isInCard(target: EventTarget | null) {
  return target instanceof Element && target.closest('[data-reaction-card]') !== null;
}

function onCardFocusOut(event: FocusEvent) {
  if (!isInCard(event.relatedTarget)) scheduleHide();
}

/** ホバーで開いたカードはフォーカスが札の外にあるので、document で受けて Esc で閉じられるようにする */
function onPeekEscape(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !peek.value) return;
  // タスク詳細の dialog まで閉じないよう止める
  event.preventDefault();
  event.stopPropagation();
  const { anchor } = peek.value;
  const focusInCard = isInCard(document.activeElement);
  hidePeekNow();
  if (focusInCard) anchor.focus();
}

// ─── 全員の一覧 ───

const panel = ref<{ emoji: string; anchor: HTMLElement } | null>(null);

function openPanel(emoji: string, anchor: HTMLElement) {
  hidePeekNow();
  panel.value = { emoji, anchor };
}

function closePanel(restoreFocus: boolean) {
  const anchor = panel.value?.anchor;
  panel.value = null;
  if (restoreFocus && anchor?.isConnected) anchor.focus();
}

/** ⋯ メニューの「リアクションを見る」のように、札の外から全員の一覧を開く */
function openPeople(emoji?: string) {
  const chips = rootRef.value?.querySelectorAll<HTMLElement>('[data-reaction-chip]') ?? [];
  const anchor = Array.from(chips).find((chip) => !emoji || chip.dataset.emoji === emoji);
  if (anchor?.dataset.emoji) openPanel(anchor.dataset.emoji, anchor);
}

defineExpose({ openPeople });

onMounted(() => document.addEventListener('keydown', onPeekEscape));

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onPeekEscape);
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
});
</script>

<template>
  <div v-if="showRow || error">
    <div
      v-if="showRow"
      ref="rootRef"
      role="group"
      aria-label="リアクション"
      class="flex flex-wrap items-center gap-1"
    >
      <template v-for="reaction in reactions" :key="reaction.emoji">
        <ReactionChip
          :emoji="reaction.emoji"
          :count="reaction.count"
          :reacted="reaction.reacted"
          :disabled="!canReact || busy"
          @toggle="onToggle(reaction.emoji)"
          @long-press="openPanel(reaction.emoji, $event)"
          @click="hidePeekNow"
          @pointerenter="onChipPointerEnter($event, reaction.emoji)"
          @pointerleave="scheduleHide"
          @focus="onChipFocus($event, reaction.emoji)"
          @blur="onChipBlur"
        />
        <!-- DOM を札の直後に置き、札の次の Tab がカードの「すべて見る」に入るようにする -->
        <AnchoredPanel
          v-if="peek?.emoji === reaction.emoji && peekReaction"
          :anchor="peek.anchor"
          data-reaction-card
          @dismiss="hidePeekNow"
          @pointerenter="cancelHide"
          @pointerleave="scheduleHide"
          @focusin="cancelHide"
          @focusout="onCardFocusOut"
        >
          <ReactionPeopleCard
            :emoji="reaction.emoji"
            :count="reaction.count"
            :users="reaction.users"
            :current-user-id="currentUserId"
            @show-all="openPanel(reaction.emoji, peek!.anchor)"
          />
        </AnchoredPanel>
      </template>
      <ReactionAddButton v-if="canReact" :selected="selected" :disabled="busy" @select="onPick" />
    </div>
    <p v-if="error" role="alert" class="mt-1 text-xs text-destructive">{{ error }}</p>
    <AnchoredPanel
      v-if="panel"
      :anchor="panel.anchor"
      dismiss-on-focus-out
      @dismiss="closePanel(false)"
    >
      <ReactionPeoplePanel
        :reactions="reactions"
        :initial-emoji="panel.emoji"
        :fetch-users="fetchUsers"
        :profile-href="profileHref"
        :current-user-id="currentUserId"
        @close="closePanel(true)"
      />
    </AnchoredPanel>
  </div>
</template>

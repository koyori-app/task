<script setup lang="ts">
import { computed } from 'vue';

import { mentionLabel, type MentionUser } from '@/lib/comment-mention';

/**
 * コメント本文のメンションの札（`@今の名`）。
 *
 * 状態で見た目を分ける: いま入れる者は強調色、本人へのメンションはさらに濃く、
 * 退会した者・このプロジェクトに入れぬ者は灰色（名前は出さず定型の句にする。id も出さない）。
 * 本文の他の字の描き方は変えぬ。後で KFM の remark プラグインへ移す時も、この札を描く部品を
 * そのまま使える。
 */
const props = defineProps<{
  /** 引けた表示情報。表に無い id は undefined（不明な利用者の灰色の札になる） */
  user: MentionUser | undefined;
  /** 本文の印の id（本人判定用。画面には出さない） */
  userId: string;
  /** ログイン中ユーザーの ID。一致すれば「己へのメンション」として強調する */
  currentUserId?: string | null;
  /** 渡せばプロフィール画面へのリンクにする（member のみ） */
  profileHref?: (userId: string) => string;
}>();

const label = computed(() => mentionLabel(props.user));
const isMember = computed(() => props.user?.status === 'member' && !!props.user.name);
const isSelf = computed(
  () =>
    isMember.value &&
    !!props.currentUserId &&
    props.currentUserId.toLowerCase() === props.userId.toLowerCase(),
);
const href = computed(() => (isMember.value ? props.profileHref?.(props.userId) : undefined));
const classes = computed(() => {
  if (!isMember.value) return 'bg-muted text-muted-foreground';
  return isSelf.value
    ? 'bg-primary/25 text-primary font-semibold'
    : 'bg-primary/10 text-primary font-medium';
});
</script>

<template>
  <component
    :is="href ? 'a' : 'span'"
    :href="href"
    class="rounded px-1"
    :class="[classes, href && 'hover:underline']"
    data-mention
    :data-mention-status="user?.status ?? 'unknown'"
    :data-mention-self="isSelf ? '' : undefined"
    >{{ label }}</component
  >
</template>

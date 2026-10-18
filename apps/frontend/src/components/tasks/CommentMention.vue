<script setup lang="ts">
import { computed } from 'vue';

import { mentionChip, type MentionUser } from '@/lib/comment-mention';
import { avatarInitials } from '@/lib/initials';
import '@/lib/remark-kfm-mention/style.css';

/**
 * コメント本文のメンションの札（アバターと `@今の名` の丸い札）。
 *
 * 見た目は説明文の KFM の札と同じ一枚（`remark-kfm-mention/style.css` の `span.kfm-mention`）を使い、
 * 頭のアバターも KFM・入力欄の札と同じ印（`UserAvatar` の xs と同じ 1rem の丸。頭文字は `data-initials`
 * を CSS で描くので、札の字・コピー・読み上げに頭文字が混ざらぬ）にする。
 * 状態の判じ（字・本人・アバターの有無と灰色）は `mentionChip` に寄せる。いま入れる者は強調色、
 * 本人へのメンションは強調色の地に反転の字、外れた者・退会した者・名の引けぬ者は灰色の字と破線の縁
 * （アバターも灰色）。名の引けぬ者の札は定型の句だけで、アバターを出さぬ。id はどれも出さない。
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

const chip = computed(() => mentionChip(props.user, props.userId, props.currentUserId));
const href = computed(() =>
  chip.value.state === 'member' && chip.value.avatar
    ? props.profileHref?.(props.userId)
    : undefined,
);
const initials = computed(() => (chip.value.avatar ? avatarInitials(chip.value.avatar.name) : ''));
const classes = computed(() => [
  'kfm-mention',
  `kfm-mention-${chip.value.state}`,
  chip.value.self && 'kfm-mention-self',
  href.value && 'hover:underline',
]);
</script>

<template>
  <component
    :is="href ? 'a' : 'span'"
    :href="href"
    :class="classes"
    data-mention
    :data-mention-status="chip.state"
    :data-mention-self="chip.self ? '' : undefined"
    ><span
      v-if="chip.avatar"
      class="kfm-mention-avatar"
      :data-initials="initials"
      aria-hidden="true"
      data-mention-avatar
      ><img v-if="chip.avatar.src" :src="chip.avatar.src" alt="" /></span
    >{{ chip.label }}</component
  >
</template>

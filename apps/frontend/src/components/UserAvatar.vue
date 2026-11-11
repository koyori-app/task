<script setup lang="ts">
import { Avatar } from '@koyori-app/ui-vue';
import { computed, type HTMLAttributes } from 'vue';

import {
  USER_AVATAR_INACTIVE_CLASS,
  USER_AVATAR_INITIALS_CLASS,
  type UserAvatarSize,
  type UserAvatarUser,
} from '@/lib/user-avatar';
import { cn } from '@/lib/utils';

interface UserAvatarProps {
  user: UserAvatarUser;
  /** 大きさの段。既定は `xl`（32px） */
  size?: UserAvatarSize;
  /** 角の形。既定は丸（`full`） */
  shape?: 'full' | 'md' | 'lg';
  /**
   * 外れた者（退会・プロジェクトの外）の灰色の表示。画像も灰色にして薄くし、
   * 「いま関われる利用者」と見分けられるようにする。
   */
  inactive?: boolean;
  /**
   * 隣に同じ名を字で出す時は true。読み上げから外し、名を二度読まぬようにする。
   */
  decorative?: boolean;
  /**
   * アバターの根に足す class（縁取り・重ねる余白・地の色等）。
   * 地の色を替える時は、その地に対して対比 4.5:1 以上の字の色を併せて付ける。
   */
  class?: HTMLAttributes['class'];
}

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<UserAvatarProps>(), {
  size: 'xl',
  shape: 'full',
  inactive: false,
  decorative: false,
  class: undefined,
});

/** 段（`USER_AVATAR_SIZES` の size-*）を koyori の直径（px）へ。頭文字の字の大きさは koyori が直径から決める。 */
const SIZE_PX: Record<UserAvatarSize, number> = {
  xs: 16,
  sm: 20,
  md: 24,
  lg: 28,
  xl: 32,
  '2xl': 56,
  '3xl': 64,
};
const SHAPES = { full: 'rounded-full', md: 'rounded-md', lg: 'rounded-lg' } as const;
/** BOT の札の字の大きさ。小さい段では丸の幅を越えぬよう字を詰める。 */
const BOT_BADGE_TEXT: Record<UserAvatarSize, string> = {
  xs: 'text-[6px] px-px',
  sm: 'text-[7px] px-px',
  md: 'text-[7px] px-0.5',
  lg: 'text-[8px] px-0.5',
  xl: 'text-[8px] px-0.5',
  '2xl': 'text-[10px] px-1',
  '3xl': 'text-xs px-1',
};

/**
 * BOT は丸の右下に札を重ねる。人の表示は根を Avatar のまま変えず、画像の比べを動かさぬ。
 * 札は Avatar を読み上げから外した時（decorative）も読む。名だけでは BOT と分からぬため。
 */
const isBot = computed(() => props.user.account_kind === 'bot');
// 空文字の avatar_url も画像なし。画像の読み込みに失敗した時は koyori が頭文字へ落とす
</script>

<template>
  <span v-if="isBot" class="relative inline-flex shrink-0">
    <Avatar
      :name="user.name ?? user.username ?? ''"
      :src="user.avatar_url || undefined"
      :size="SIZE_PX[size]"
      :class="
        cn(
          USER_AVATAR_INITIALS_CLASS,
          SHAPES[shape],
          inactive && USER_AVATAR_INACTIVE_CLASS,
          props.class,
        )
      "
      :aria-hidden="decorative || undefined"
      v-bind="$attrs"
    />
    <span
      role="img"
      aria-label="BOT アカウント"
      :class="
        cn(
          'absolute -right-1 -bottom-1 rounded-sm bg-foreground leading-none font-semibold text-background ring-1 ring-background',
          BOT_BADGE_TEXT[size],
        )
      "
      >BOT</span
    >
  </span>
  <Avatar
    v-else
    :name="user.name ?? user.username ?? ''"
    :src="user.avatar_url || undefined"
    :size="SIZE_PX[size]"
    :class="
      cn(
        USER_AVATAR_INITIALS_CLASS,
        SHAPES[shape],
        inactive && USER_AVATAR_INACTIVE_CLASS,
        props.class,
      )
    "
    :aria-hidden="decorative || undefined"
    v-bind="$attrs"
  />
</template>

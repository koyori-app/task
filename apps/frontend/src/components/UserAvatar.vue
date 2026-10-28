<script setup lang="ts">
import { Avatar } from '@koyori-app/ui-vue';
import type { HTMLAttributes } from 'vue';

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
// 空文字の avatar_url も画像なし。画像の読み込みに失敗した時は koyori が頭文字へ落とす
</script>

<template>
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
  />
</template>

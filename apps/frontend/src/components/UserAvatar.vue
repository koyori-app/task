<script setup lang="ts">
import type { HTMLAttributes } from 'vue';
import { computed } from 'vue';

import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { avatarInitials } from '@/lib/initials';
import {
  USER_AVATAR_INACTIVE_CLASS,
  USER_AVATAR_INITIALS_CLASS,
  USER_AVATAR_SIZES,
  type UserAvatarSize,
  type UserAvatarUser,
} from '@/lib/user-avatar';
import { cn } from '@/lib/utils';

interface UserAvatarProps {
  user: UserAvatarUser;
  /** 大きさの段。既定は `xl`（size-8） */
  size?: UserAvatarSize;
  /** 角の形。既定は丸（`full`） */
  shape?: 'full' | 'md' | 'lg';
  /**
   * 外れた者（退会・プロジェクトの外）の灰色の表示。画像も灰色にして薄くし、
   * 「いま関われる利用者」と見分けられるようにする。
   */
  inactive?: boolean;
  /**
   * 隣に同じ名を字で出す時は true。画像の alt を空にして、読み上げが名を二度読まぬようにする。
   */
  decorative?: boolean;
  /** 頭文字に使う字数（コードポイント単位）。既定は 2 */
  initialLength?: number;
  /** 頭文字を名から作らず、この字に替える（+N のチップ等） */
  fallbackText?: string;
  /**
   * 頭文字の見た目だけを足す（地の色・太さ等）。字の大きさと既定の字の色は段で決まるので、ここでは替えぬ。
   * 地の色を替える時は、その地に対して対比 4.5:1 以上の字の色を併せて付ける。
   */
  fallbackClass?: HTMLAttributes['class'];
  fallbackStyle?: HTMLAttributes['style'];
  /** アバターの根に足す class（縁取り・重ねる余白等の配置用） */
  class?: HTMLAttributes['class'];
}

const props = withDefaults(defineProps<UserAvatarProps>(), {
  size: 'xl',
  shape: 'full',
  inactive: false,
  decorative: false,
  initialLength: 2,
  fallbackText: undefined,
  fallbackClass: undefined,
  fallbackStyle: undefined,
  class: undefined,
});

const name = computed(() => props.user.name ?? props.user.username ?? '');
/** 空文字も「画像なし」。画像の読み込みに失敗した時は reka-ui が頭文字へ落とす。 */
const src = computed(() => props.user.avatar_url || undefined);
const text = computed(() => props.fallbackText ?? avatarInitials(name.value, props.initialLength));

const SHAPES = { full: 'rounded-full', md: 'rounded-md', lg: 'rounded-lg' } as const;
</script>

<template>
  <Avatar
    :class="
      cn(
        USER_AVATAR_SIZES[size],
        SHAPES[shape],
        inactive && USER_AVATAR_INACTIVE_CLASS,
        props.class,
      )
    "
  >
    <AvatarImage v-if="src" :src="src" :alt="decorative ? '' : name" />
    <AvatarFallback
      :class="cn(SHAPES[shape], USER_AVATAR_INITIALS_CLASS, fallbackClass)"
      :style="fallbackStyle"
    >
      {{ text }}
    </AvatarFallback>
  </Avatar>
</template>

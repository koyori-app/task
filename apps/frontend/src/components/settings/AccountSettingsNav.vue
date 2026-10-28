<script setup lang="ts">
import {
  PhBell,
  PhDevices,
  PhFlask,
  PhKey,
  PhShieldCheck,
  PhSlidersHorizontal,
  PhUser,
} from '@phosphor-icons/vue';
import { computed, type Component } from 'vue';
import { usePreference } from '@/composables/useLocalSettings';

type NavItem = {
  title: string;
  icon: Component;
  /** 未実装の項目は href を持たず、押せない見出しとして並べる。 */
  href?: string;
};

const items: NavItem[] = [
  { title: 'プロフィール', icon: PhUser, href: '/settings/profile' },
  { title: '環境設定', icon: PhSlidersHorizontal, href: '/settings/preferences' },
  { title: '通知', icon: PhBell },
  { title: 'セキュリティ', icon: PhShieldCheck, href: '/settings/security' },
  { title: 'アクセストークン', icon: PhKey, href: '/settings/tokens' },
  { title: 'セッション', icon: PhDevices },
];

/** 環境設定で表示を有効にした時だけ、一覧の一番下（区切り線の下）に出す。 */
const experimentsItem: NavItem = {
  title: '実験的機能',
  icon: PhFlask,
  href: '/settings/experiments',
};

// 残った値は SSR で読めぬので、mount の後に出す（usePreference が mount の前は既定値＝無効を返す）
const showExperiments = usePreference('showExperiments');
const groups = computed(() => (showExperiments.value ? [items, [experimentsItem]] : [items]));

defineProps<{ currentPath: string }>();
</script>

<template>
  <nav class="flex flex-col gap-1" aria-label="アカウント設定">
    <template v-for="(group, groupIndex) in groups" :key="groupIndex">
      <hr v-if="groupIndex > 0" class="koyori-separator my-1" data-nav-separator />
      <template v-for="item in group" :key="item.title">
        <a
          v-if="item.href"
          :href="item.href"
          :aria-current="item.href === currentPath ? 'page' : undefined"
          class="flex items-center gap-2 rounded-md px-3 py-2 text-sm aria-[current=page]:bg-accent aria-[current=page]:font-medium hover:bg-accent/50"
        >
          <component :is="item.icon" class="size-4 shrink-0" />
          <span>{{ item.title }}</span>
        </a>
        <span
          v-else
          aria-disabled="true"
          title="未実装です"
          class="text-muted-foreground/50 flex cursor-not-allowed items-center gap-2 rounded-md px-3 py-2 text-sm"
        >
          <component :is="item.icon" class="size-4 shrink-0" />
          <span>{{ item.title }}</span>
        </span>
      </template>
    </template>
  </nav>
</template>

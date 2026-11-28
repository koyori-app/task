<script setup lang="ts">
import { UserPanel, type UserPanelSection } from '@koyori-app/ui-vue';
import { PhDownloadSimple, PhGear, PhPencilSimple, PhSignOut } from '@phosphor-icons/vue';
import { usePageContext } from 'vike-vue/usePageContext';
import { computed } from 'vue';

import { useInstallPrompt } from '@/lib/pwa/install-prompt';

/**
 * サイドバー下端のユーザー。パネルの行・プロフィールカード・top layer・フォーカス・閉じ方は ui の UserPanel が持つ。
 * ここはカードの中身（項目とアイコン）と、install・ログアウトの配線だけを持つ。
 */
const props = defineProps<{
  user: {
    name: string;
    email: string;
    avatar: string;
  };
  onLogout?: () => void | Promise<void>;
  /** サイドバーを畳んだ状態。アバターだけを出し、歯車は出さない */
  rail?: boolean;
}>();

const pageContext = usePageContext();

/** アカウント設定（/settings 配下）を見ているあいだは歯車に印を付ける。 */
const isAccountSettings = computed(() => /^\/settings(\/|$)/.test(pageContext.urlPathname));

// ブラウザが install を差し出した時だけ出る（iOS の Safari などでは出ない）。
const { canInstall, promptInstall } = useInstallPrompt();

const items = computed<UserPanelSection[]>(() => [
  {
    items: [
      { value: 'profile', label: 'プロフィールを編集', href: '/settings/profile', icon: true },
      ...(canInstall.value
        ? [{ value: 'install', label: 'アプリをインストール', icon: true }]
        : []),
    ],
  },
  { items: [{ value: 'logout', label: 'ログアウト', destructive: true, icon: true }] },
]);

const itemIcons = {
  profile: PhPencilSimple,
  install: PhDownloadSimple,
  logout: PhSignOut,
} as const;

function onSelect(value: string) {
  // profile は <a href> の遷移に任せる（Sidebar の closeOnNavigate が狭い画面の Drawer を閉じる）
  if (value === 'install') void promptInstall();
  else if (value === 'logout') void props.onLogout?.();
}
</script>

<template>
  <UserPanel
    :name="user.name"
    :description="user.email || undefined"
    :src="user.avatar || undefined"
    card-label="アカウント"
    :items="items"
    :compact="rail"
    :on-select="onSelect"
  >
    <template #item-icon="{ item }">
      <component :is="itemIcons[item.value as keyof typeof itemIcons]" :size="16" />
    </template>
    <template #action>
      <a
        href="/settings/profile"
        aria-label="アカウント設定"
        title="アカウント設定"
        class="flex size-7.5 items-center justify-center rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
        :class="isAccountSettings ? 'bg-sidebar-accent text-sidebar-accent-foreground' : ''"
        :aria-current="isAccountSettings ? 'page' : undefined"
      >
        <PhGear class="size-4" />
      </a>
    </template>
  </UserPanel>
</template>

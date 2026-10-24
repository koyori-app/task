<script setup lang="ts">
import { Dropdown, type DropdownSection } from '@koyori-app/ui-vue';
import { navigate } from 'vike/client/router';
import { computed } from 'vue';

import UserAvatar from '@/components/UserAvatar.vue';
import { useInstallPrompt } from '@/lib/pwa/install-prompt';

const props = defineProps<{
  user: {
    name: string;
    email: string;
    avatar: string;
  };
  onLogout?: () => void | Promise<void>;
}>();

const avatarUser = computed(() => ({ name: props.user.name, avatar_url: props.user.avatar }));

// ブラウザが install を差し出した時だけ出る（iOS の Safari などでは出ない）。
const { canInstall, promptInstall } = useInstallPrompt();

const sections = computed<DropdownSection[]>(() => [
  { heading: props.user.email, items: [{ value: 'upgrade', label: 'Upgrade to Pro' }] },
  {
    items: [
      { value: 'account', label: 'Account' },
      { value: 'billing', label: 'Billing' },
      { value: 'notifications', label: 'Notifications' },
    ],
  },
  ...(canInstall.value ? [{ items: [{ value: 'install', label: 'Install app' }] }] : []),
  { items: [{ value: 'logout', label: 'Log out', destructive: true }] },
]);

function onSelect(value: string) {
  if (value === 'account') void navigate('/settings/profile');
  else if (value === 'install') void promptInstall();
  else if (value === 'logout') void props.onLogout?.();
}
</script>

<template>
  <Dropdown :label="user.name" :items="sections" :on-select="onSelect">
    <template #trigger>
      <span class="flex max-w-45 items-center gap-2 text-left">
        <UserAvatar :user="avatarUser" size="md" shape="md" />
        <span class="truncate font-medium">{{ user.name }}</span>
      </span>
    </template>
  </Dropdown>
</template>

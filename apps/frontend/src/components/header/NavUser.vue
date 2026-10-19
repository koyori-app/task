<script setup lang="ts">
import {
  PhSealCheck,
  PhBell,
  PhCaretUpDown,
  PhCreditCard,
  PhDownloadSimple,
  PhSignOut,
  PhSparkle,
} from '@phosphor-icons/vue';

import UserAvatar from '@/components/UserAvatar.vue';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { computed } from 'vue';
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
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger as-child>
      <button
        type="button"
        class="flex max-w-45 items-center gap-2 rounded-md px-1.5 py-1 text-left text-sm hover:bg-accent data-[state=open]:bg-accent"
      >
        <UserAvatar :user="avatarUser" size="md" shape="md" />
        <span class="truncate font-medium">{{ user.name }}</span>
        <PhCaretUpDown class="size-4 shrink-0 text-muted-foreground" />
      </button>
    </DropdownMenuTrigger>
    <DropdownMenuContent class="min-w-56 rounded-lg" side="bottom" align="end" :side-offset="4">
      <DropdownMenuLabel class="p-0 font-normal">
        <div class="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
          <UserAvatar :user="avatarUser" size="xl" shape="lg" />
          <div class="grid flex-1 text-left text-sm leading-tight">
            <span class="truncate font-semibold">{{ user.name }}</span>
            <span class="truncate text-xs">{{ user.email }}</span>
          </div>
        </div>
      </DropdownMenuLabel>
      <DropdownMenuSeparator />
      <DropdownMenuGroup>
        <DropdownMenuItem>
          <PhSparkle />
          Upgrade to Pro
        </DropdownMenuItem>
      </DropdownMenuGroup>
      <DropdownMenuSeparator />
      <DropdownMenuGroup>
        <DropdownMenuItem as-child>
          <a href="/settings/profile">
            <PhSealCheck />
            Account
          </a>
        </DropdownMenuItem>
        <DropdownMenuItem>
          <PhCreditCard />
          Billing
        </DropdownMenuItem>
        <DropdownMenuItem>
          <PhBell />
          Notifications
        </DropdownMenuItem>
      </DropdownMenuGroup>
      <template v-if="canInstall">
        <DropdownMenuSeparator />
        <DropdownMenuItem data-testid="install-app" @click="promptInstall()">
          <PhDownloadSimple />
          Install app
        </DropdownMenuItem>
      </template>
      <DropdownMenuSeparator />
      <DropdownMenuItem @click="onLogout?.()">
        <PhSignOut />
        Log out
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
</template>

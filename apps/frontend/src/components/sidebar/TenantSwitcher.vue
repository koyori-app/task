<script setup lang="ts">
import { PhBuildings, PhCaretUpDown, PhPlus, PhWarningCircle } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import CreateTenantDialog from './CreateTenantDialog.vue';
import type { Tenant } from '@/stores/tenant';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from '@/components/ui/sidebar';

const props = defineProps<{
  tenants: Tenant[];
  selectedTenantId: string | null;
  loading?: boolean;
  error?: string | null;
}>();

const emit = defineEmits<{
  select: [tenant: Tenant];
  retry: [];
}>();

const { isMobile } = useSidebar();
const createDialogOpen = ref(false);
const activeTenant = computed(
  () => props.tenants.find((tenant) => tenant.id === props.selectedTenantId) ?? null,
);
const tenantNotFound = computed(
  () => !props.loading && !props.error && props.tenants.length > 0 && !activeTenant.value,
);
</script>

<template>
  <SidebarMenu>
    <SidebarMenuItem>
      <DropdownMenu>
        <DropdownMenuTrigger as-child>
          <SidebarMenuButton
            size="lg"
            :disabled="loading"
            class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
          >
            <div
              class="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground"
            >
              <img
                v-if="activeTenant?.icon_url"
                :src="activeTenant.icon_url"
                alt=""
                class="size-5 rounded object-cover"
              />
              <PhBuildings v-else class="size-4" />
            </div>
            <div class="grid flex-1 text-left text-sm leading-tight">
              <span class="truncate font-medium">
                {{
                  loading
                    ? 'テナントを読み込み中…'
                    : tenantNotFound
                      ? '指定されたテナントが見つかりません'
                      : (activeTenant?.name ?? '所属テナントなし')
                }}
              </span>
              <span class="truncate text-xs">{{
                tenantNotFound
                  ? 'URLを確認してください'
                  : (activeTenant?.display_id ?? '利用可能なテナントがありません')
              }}</span>
            </div>
            <PhCaretUpDown class="ml-auto" />
          </SidebarMenuButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          class="w-(--reka-dropdown-menu-trigger-width) min-w-56 rounded-lg"
          align="start"
          :side="isMobile ? 'bottom' : 'right'"
          :side-offset="4"
        >
          <DropdownMenuLabel class="text-xs text-muted-foreground"> テナント </DropdownMenuLabel>
          <DropdownMenuItem v-if="error" class="gap-2 p-2 text-destructive" @click="emit('retry')">
            <PhWarningCircle class="size-4" />
            {{ error }}（再試行）
          </DropdownMenuItem>
          <DropdownMenuItem v-else-if="!loading && tenants.length === 0" disabled class="p-2">
            所属テナントがありません
          </DropdownMenuItem>
          <DropdownMenuItem
            v-for="tenant in tenants"
            :key="tenant.id"
            class="gap-2 p-2"
            @click="emit('select', tenant)"
          >
            <div class="flex size-6 items-center justify-center rounded-sm border">
              <img
                v-if="tenant.icon_url"
                :src="tenant.icon_url"
                alt=""
                class="size-4 rounded object-cover"
              />
              <PhBuildings v-else class="size-3.5 shrink-0" />
            </div>
            {{ tenant.name }}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem class="gap-2 p-2" @select="createDialogOpen = true">
            <div class="flex size-6 items-center justify-center rounded-md border bg-transparent">
              <PhPlus class="size-4" />
            </div>
            <div class="font-medium text-muted-foreground">Add tenant</div>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </SidebarMenuItem>
  </SidebarMenu>
  <CreateTenantDialog v-model:open="createDialogOpen" />
</template>

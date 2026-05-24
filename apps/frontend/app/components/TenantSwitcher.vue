<script setup lang="ts">
import { PhBuildings, PhCaretUpDown, PhPlus } from '@phosphor-icons/vue';
import { ref, watch } from 'vue';
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
  tenants: {
    id: string;
    name: string;
    display_id: string;
  }[];
}>();

const emit = defineEmits<{
  (e: 'change', tenantId: string): void;
}>();

const { isMobile } = useSidebar();
const activeTenant = ref(props.tenants[0] ?? null);

watch(
  () => props.tenants,
  (list) => {
    if (!activeTenant.value && list.length > 0) {
      activeTenant.value = list[0]!;
    } else if (
      activeTenant.value &&
      !list.find((t) => t.id === activeTenant.value!.id)
    ) {
      activeTenant.value = list[0] ?? null;
    }
  },
);

function select(tenant: (typeof props.tenants)[number]) {
  activeTenant.value = tenant;
  emit('change', tenant.id);
}
</script>

<template>
  <SidebarMenu>
    <SidebarMenuItem>
      <DropdownMenu>
        <DropdownMenuTrigger as-child>
          <SidebarMenuButton
            size="lg"
            class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
          >
            <div
              class="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground"
            >
              <PhBuildings class="size-4" />
            </div>
            <div class="grid flex-1 text-left text-sm leading-tight">
              <span class="truncate font-medium">
                {{ activeTenant?.name ?? 'テナントなし' }}
              </span>
              <span class="truncate text-xs">{{ activeTenant?.display_id ?? '' }}</span>
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
          <DropdownMenuLabel class="text-xs text-muted-foreground">テナント</DropdownMenuLabel>
          <DropdownMenuItem
            v-for="tenant in tenants"
            :key="tenant.id"
            class="gap-2 p-2"
            @click="select(tenant)"
          >
            <div class="flex size-6 items-center justify-center rounded-sm border">
              <PhBuildings class="size-3.5 shrink-0" />
            </div>
            {{ tenant.name }}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem class="gap-2 p-2" disabled>
            <div class="flex size-6 items-center justify-center rounded-md border bg-transparent">
              <PhPlus class="size-4" />
            </div>
            <div class="font-medium text-muted-foreground">テナントを追加</div>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </SidebarMenuItem>
  </SidebarMenu>
</template>

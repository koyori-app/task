<script setup lang="ts">
import type { LucideIcon } from '@lucide/vue';
import { SidebarLink } from '@koyori-app/ui-vue';

defineProps<{
  items: {
    title: string;
    url: string;
    icon?: LucideIcon;
    isActive?: boolean;
  }[];
}>();
</script>

<template>
  <div class="flex flex-col gap-(--koyori-space-xs)">
    <!-- レールでは見出しの文字が幅に収まらないので、読み上げにだけ残す -->
    <h2
      class="px-3 py-1.5 text-xs font-medium text-muted-foreground in-data-[sidebar-rail=true]:sr-only"
    >
      Platform
    </h2>
    <SidebarLink
      v-for="item in items"
      :key="item.title"
      :label="item.title"
      :href="item.url"
      :current="item.isActive"
    >
      <template v-if="item.icon" #icon><component :is="item.icon" class="size-4" /></template>
    </SidebarLink>
  </div>
</template>

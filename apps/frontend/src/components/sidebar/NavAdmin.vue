<script setup lang="ts">
import { SidebarLink } from '@koyori-app/ui-vue';

defineProps<{ currentPath: string }>();

const items = [
  { title: 'ユーザー', url: '/admin/users' },
  { title: 'システム設定', url: '/admin/settings' },
];
</script>

<template>
  <div class="flex flex-col gap-(--koyori-space-xs)">
    <h2
      class="px-3 py-1.5 text-xs font-medium text-muted-foreground in-data-[sidebar-rail=true]:sr-only"
    >
      管理
    </h2>
    <SidebarLink
      v-for="item in items"
      :key="item.url"
      :label="item.title"
      :href="item.url"
      :current="currentPath.startsWith(item.url)"
    />
  </div>
</template>

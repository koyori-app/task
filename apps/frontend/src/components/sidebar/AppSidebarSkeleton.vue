<script setup lang="ts">
import { Skeleton } from '@/components/ui/skeleton';
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from '@/components/ui/sidebar';
</script>

<template>
  <Sidebar collapsible="icon">
    <SidebarHeader class="p-2">
      <Skeleton class="h-10 w-full rounded-lg" />
    </SidebarHeader>
    <SidebarContent class="gap-4 p-2">
      <div class="space-y-2">
        <Skeleton class="h-8 w-full" />
        <Skeleton class="h-8 w-5/6" />
        <Skeleton class="h-8 w-4/6" />
      </div>
      <div class="space-y-2">
        <Skeleton class="h-4 w-20" />
        <Skeleton class="h-8 w-full" />
        <Skeleton class="h-8 w-full" />
        <Skeleton class="h-8 w-3/4" />
      </div>
    </SidebarContent>
    <SidebarFooter class="p-2">
      <div class="flex items-center gap-2">
        <Skeleton class="size-8 shrink-0 rounded-full" />
        <div class="min-w-0 flex-1 space-y-1.5">
          <Skeleton class="h-3 w-24" />
          <Skeleton class="h-3 w-32" />
        </div>
      </div>
    </SidebarFooter>
    <SidebarRail />
  </Sidebar>
</template>

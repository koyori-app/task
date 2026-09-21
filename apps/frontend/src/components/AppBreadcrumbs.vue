<script setup lang="ts">
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb';
import type { BreadcrumbState } from '@/lib/breadcrumbs';

defineProps<BreadcrumbState>();
</script>

<template>
  <div class="flex h-6 min-w-0 flex-1 items-center" data-app-breadcrumbs>
    <div
      v-if="loading"
      role="status"
      aria-label="パンくずを読み込み中"
      class="flex h-5 w-64 max-w-full items-center gap-2"
    >
      <span
        v-for="index in 3"
        :key="index"
        aria-hidden="true"
        class="h-4 flex-1 animate-pulse rounded bg-muted"
      />
    </div>
    <Breadcrumb v-else-if="segments.length" aria-label="パンくず" class="min-w-0 max-w-full">
      <BreadcrumbList class="flex-nowrap gap-1 overflow-x-auto whitespace-nowrap sm:gap-2.5">
        <template v-for="(segment, index) in segments" :key="segment.href">
          <BreadcrumbSeparator v-if="index" class="shrink-0" />
          <BreadcrumbItem class="min-w-0 shrink-0">
            <span
              v-if="segment.current"
              aria-current="page"
              :title="segment.name"
              class="block max-w-40 truncate text-foreground sm:max-w-64"
              >{{ segment.name }}</span
            >
            <BreadcrumbLink
              v-else
              :href="segment.href"
              :title="segment.name"
              class="block max-w-40 truncate sm:max-w-64"
            >
              {{ segment.name }}
            </BreadcrumbLink>
          </BreadcrumbItem>
        </template>
      </BreadcrumbList>
    </Breadcrumb>
  </div>
</template>

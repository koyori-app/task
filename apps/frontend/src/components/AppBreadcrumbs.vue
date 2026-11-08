<script setup lang="ts">
import { Breadcrumb } from '@koyori-app/ui-vue';
import { computed } from 'vue';

import type { BreadcrumbState } from '@/lib/breadcrumbs';

const props = defineProps<BreadcrumbState>();

/** 現在地（最後の段）はリンクにしない */
const items = computed(() =>
  props.segments.map((segment) => ({
    label: segment.name,
    href: segment.current ? undefined : segment.href,
  })),
);
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
    <!--
      koyori は折り返すが、ヘッダーの高さを保つため一列に並べ、狭い画面では横にスクロールさせる。
      段が縮んで字が重ならぬよう、段は縮めない。
    -->
    <Breadcrumb
      v-else-if="segments.length"
      label="パンくず"
      :items="items"
      class="min-w-0 max-w-full [&_li]:shrink-0 [&_ol]:flex-nowrap [&_ol]:overflow-x-auto [&_ol]:whitespace-nowrap"
    />
  </div>
</template>

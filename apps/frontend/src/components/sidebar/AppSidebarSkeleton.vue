<script setup lang="ts">
import { Sidebar, Skeleton } from '@koyori-app/ui-vue';

defineProps<{
  /** 読み込み後の AppSidebar と同じ幅で出すため、呼び出し側の rail を受け取る */
  rail?: boolean;
}>();
</script>

<template>
  <Sidebar id="app-sidebar" label="ナビゲーション" :rail="rail">
    <div class="flex flex-col gap-2">
      <Skeleton />
      <Skeleton width="83%" />
      <Skeleton width="67%" />
    </div>
    <div class="mt-4 flex flex-col gap-2">
      <Skeleton width="5rem" height="1rem" />
      <Skeleton />
      <Skeleton />
      <Skeleton width="75%" />
    </div>
  </Sidebar>
</template>

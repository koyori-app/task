<script setup lang="ts">
import { TanStackDevtools } from '@tanstack/vue-devtools';
import type { TanStackDevtoolsVuePlugin } from '@tanstack/vue-devtools';
import { VueQueryDevtoolsPanel } from '@tanstack/vue-query-devtools';

const plugins: TanStackDevtoolsVuePlugin[] = [
  { name: 'Vue Query', component: VueQueryDevtoolsPanel },
];
</script>

<template>
  <TanStackDevtools :plugins="plugins" />
</template>

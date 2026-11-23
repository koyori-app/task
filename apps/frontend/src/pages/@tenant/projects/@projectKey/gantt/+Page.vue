<script setup lang="ts">
import { computed } from 'vue';
import { usePageContext } from 'vike-vue/usePageContext';

import ScheduleGantt from '@/components/gantt/ScheduleGantt.vue';

const pageContext = usePageContext();
const projectKey = computed(() => String(pageContext.routeParams.projectKey ?? ''));
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-4">
    <h1 class="text-2xl font-semibold tracking-tight">ガント</h1>
    <ScheduleGantt :project-key="projectKey" />
  </div>
</template>

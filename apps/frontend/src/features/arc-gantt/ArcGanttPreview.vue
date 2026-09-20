<script setup lang="ts">
import { GanttChart, type GanttDep, type GanttTask } from '@koyori-app/arc-vue';
import { onMounted, onUnmounted, ref } from 'vue';
import { loadArcGanttData } from './model';

const props = defineProps<{
  tenantId: string;
  projectId: string;
}>();

const tasks = ref<GanttTask[]>([]);
const deps = ref<GanttDep[]>([]);
const loading = ref(true);
const error = ref('');
const controller = new AbortController();

onMounted(async () => {
  try {
    const data = await loadArcGanttData(
      import.meta.env.VITE_API_BASE ?? '/api',
      props.tenantId,
      props.projectId,
      controller.signal,
    );
    tasks.value = data.tasks;
    deps.value = data.deps;
  } catch (cause) {
    if (!controller.signal.aborted) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    }
  } finally {
    loading.value = false;
  }
});

onUnmounted(() => controller.abort());
</script>

<template>
  <section class="rounded-md border p-4" data-testid="arc-gantt-preview">
    <h2 class="mb-3 text-sm font-semibold">Gantt preview</h2>
    <p v-if="loading" class="text-sm text-muted-foreground">Loading tasks…</p>
    <p v-else-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
    <p v-else-if="tasks.length === 0" class="text-sm text-muted-foreground">No tasks to display.</p>
    <GanttChart v-else :tasks="tasks" :deps="deps" />
  </section>
</template>

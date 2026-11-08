<script setup lang="ts">
import ScheduleGantt from '@/components/gantt/ScheduleGantt.vue';
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">ガント</h1>
      <p class="text-sm text-muted-foreground">タスクの予定を棒で見る（2 週間）</p>
    </div>
    <ScheduleGantt />
  </div>
</template>

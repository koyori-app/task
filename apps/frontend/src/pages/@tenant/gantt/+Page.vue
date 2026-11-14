<script setup lang="ts">
import ScheduleGantt from '@/components/gantt/ScheduleGantt.vue';
</script>

<template>
  <div class="flex w-full min-w-0 flex-col gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">ガント</h1>
      <p class="text-sm text-muted-foreground">
        すべてのプロジェクトのタスクを、予定開始から目安の期限までの棒と、厳守の期限の菱形で見る
      </p>
    </div>
    <ScheduleGantt />
  </div>
</template>

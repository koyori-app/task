<script setup lang="ts">
import { KEY_HELP } from '@/lib/task-query/suggest';
</script>
<template>
  <section aria-label="条件式の書き方" class="rounded border bg-background p-4 shadow-lg">
    <h3 class="mb-2 font-medium">条件式の書き方</h3>
    <p class="text-sm">空白で AND、カンマで OR、先頭の - で除外、"…" で一続きの語を指定します。</p>
    <dl class="my-2 grid grid-cols-2 gap-2 text-xs">
      <template v-for="(help, key) in KEY_HELP" :key="key"
        ><dt>{{ key }}:　{{ help[0] }}</dt>
        <dd>{{ help[1] }}</dd></template
      >
    </dl>
    <p class="text-xs text-muted-foreground">括弧と OR は使えません</p>
  </section>
</template>

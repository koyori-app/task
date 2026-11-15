<script setup lang="ts">
import type { components } from '@/generated/api';
defineProps<{ segments: components['schemas']['MatchSegment'][] }>();
</script>
<template>
  <template v-for="(segment, index) in segments" :key="index"
    ><mark v-if="segment.hit" class="rounded-sm bg-muted text-inherit">{{ segment.text }}</mark
    ><template v-else>{{ segment.text }}</template></template
  >
</template>

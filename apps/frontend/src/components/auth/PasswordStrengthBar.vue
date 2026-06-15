<script setup lang="ts">
import type { PasswordStrength } from '@/composables/usePasswordStrength';
import { computed } from 'vue';

const props = defineProps<{
  strength: PasswordStrength;
}>();

const label = computed(() => {
  switch (props.strength) {
    case 'low':
      return '弱い';
    case 'medium':
      return '普通';
    case 'high':
      return '強い';
    default:
      return '';
  }
});

function segmentClass(index: number): string {
  const base = 'h-1 flex-1 rounded-full transition-colors';
  const inactive = 'bg-muted';

  if (!props.strength) return `${base} ${inactive}`;

  const level = props.strength === 'low' ? 1 : props.strength === 'medium' ? 2 : 3;
  if (index >= level) return `${base} ${inactive}`;

  const colors = ['bg-red-500', 'bg-yellow-500', 'bg-green-500'];
  return `${base} ${colors[index]}`;
}
</script>

<template>
  <div v-if="strength" class="flex flex-col gap-1">
    <div
      class="flex gap-1"
      role="meter"
      :aria-valuenow="strength === 'low' ? 1 : strength === 'medium' ? 2 : 3"
      aria-valuemin="1"
      aria-valuemax="3"
      :aria-label="`パスワード強度: ${label}`"
    >
      <div :class="segmentClass(0)" />
      <div :class="segmentClass(1)" />
      <div :class="segmentClass(2)" />
    </div>
    <span class="text-muted-foreground text-xs">{{ label }}</span>
  </div>
</template>

<script setup lang="ts">
import { useHydrated } from '@/composables/useHydrated';

const emit = defineEmits<{
  submit: [event: SubmitEvent];
}>();

const isHydrated = useHydrated();

function handleSubmit(event: SubmitEvent) {
  if (!isHydrated.value) {
    event.preventDefault();
    return;
  }

  emit('submit', event);
}

function preventPrehydrationEnter(event: KeyboardEvent) {
  if (!isHydrated.value) {
    event.preventDefault();
  }
}
</script>

<template>
  <form
    :onsubmit.attr="isHydrated ? null : 'return false;'"
    @submit.prevent="handleSubmit"
    @keydown.enter="preventPrehydrationEnter"
  >
    <slot :is-hydrated="isHydrated" />
  </form>
</template>

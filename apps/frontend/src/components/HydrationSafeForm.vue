<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useHydrated } from '@/composables/useHydrated';

const emit = defineEmits<{
  submit: [event: SubmitEvent];
}>();

const isHydrated = useHydrated();
const formEl = ref<HTMLFormElement | null>(null);

onMounted(() => {
  formEl.value?.removeAttribute('onsubmit');
});

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
    ref="formEl"
    onsubmit="return false;"
    @submit.prevent="handleSubmit"
    @keydown.enter="preventPrehydrationEnter"
  >
    <slot :is-hydrated="isHydrated" />
  </form>
</template>

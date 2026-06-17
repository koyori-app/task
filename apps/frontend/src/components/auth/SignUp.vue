<script setup lang="ts">
import SignUpForm from './SignUpForm.vue';

defineOptions({ name: 'SignUp' });
</script>

<template>
  <div class="bg-muted flex min-h-svh flex-col items-center justify-center p-6 md:p-10">
    <div class="w-full max-w-sm md:max-w-4xl">
      <SignUpForm />
    </div>
  </div>
</template>

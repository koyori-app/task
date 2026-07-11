<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { computed } from 'vue';
import PasswordResetCompleteForm from './PasswordResetCompleteForm.vue';
import PasswordResetRequestForm from './PasswordResetRequestForm.vue';

defineOptions({ name: 'PasswordReset' });

const pageContext = usePageContext();
// メール内リンクは /auth/reset-password?token=… （backend の password_reset_email_delivery が生成）
const token = computed(() => {
  const search = (pageContext as { urlParsed?: { search?: Record<string, string> } }).urlParsed
    ?.search;
  return search?.token ?? null;
});
</script>

<template>
  <div class="bg-muted flex min-h-svh flex-col items-center justify-center p-6 md:p-10">
    <div class="w-full max-w-sm">
      <PasswordResetCompleteForm v-if="token" :token="token" />
      <PasswordResetRequestForm v-else />
    </div>
  </div>
</template>

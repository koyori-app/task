<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { computed } from 'vue';
import TwoFactor from '@/components/auth/TwoFactor.vue';

const pageContext = usePageContext();
// パスワードログインは requires_2fa で、OAuth コールバックは redirect_after 付きでここへ来る
const redirectAfter = computed(() => {
  const search = (pageContext as { urlParsed?: { search?: Record<string, string> } }).urlParsed
    ?.search;
  return search?.redirect_after ?? null;
});
</script>

<template>
  <TwoFactor :redirect-after="redirectAfter" />
</template>

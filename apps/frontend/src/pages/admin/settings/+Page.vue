<script setup lang="ts">
import AdminShell from '@/components/admin/AdminShell.vue';
import AdminRegistrationSettings from '@/components/admin/AdminRegistrationSettings.vue';
</script>

<template>
  <AdminShell title="システム設定" description="Task 全体の設定です。">
    <AdminRegistrationSettings />
  </AdminShell>
</template>

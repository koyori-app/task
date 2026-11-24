<script setup lang="ts">
import TenantCreateTask from '@/components/tasks/TenantCreateTask.vue';
</script>

<template>
  <slot />
  <!-- テナント配下のどのページからも T でタスクを作れるようにする（画面が自前の T を持てばそちらが優先） -->
  <TenantCreateTask />
</template>

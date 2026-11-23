<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import AdminShell from '@/components/admin/AdminShell.vue';
import AdminUserDetail from '@/components/admin/AdminUserDetail.vue';

const pageContext = usePageContext();
</script>

<template>
  <AdminShell title="ユーザー詳細">
    <AdminUserDetail
      :key="String(pageContext.routeParams.userId)"
      :user-id="String(pageContext.routeParams.userId)"
    />
  </AdminShell>
</template>
